/*
 * The MIT License
 * Copyright (c) 2022 Guillem Castro
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

use std::path::Path;

use crate::filesystem::StorageDriver;
use crate::ipc;
use crate::ipc::Action;
use crate::ipc::ConsumerChannel;
use crate::syscall;
use crate::filesystem;

use color_eyre::Result;

pub struct Runtime {
    // ID of the container
    ID: String,
    // Hostname
    hostname: String,
    /// Root filesystem of the container
    fs: Box<dyn StorageDriver>,
    consumer_channel: ConsumerChannel,
}

impl Runtime {

    /// Create a new container runtime
    /// # Arguments
    /// * `ID` - ID of the container
    /// * `fs` - Root filesystem driver
    /// * `consumer_channel` - Channel for receiving IPC messages
    pub fn new(id: String, fs: Box<dyn StorageDriver>, consumer_channel: ConsumerChannel) -> Runtime {
        Runtime {
            ID: id,
            hostname: "".to_string(),
            fs: fs,
            consumer_channel: consumer_channel
        }
    }

    /// Execute the container
    pub fn run(&mut self) -> Result<()> {
        // Mount first the rootfs as private so the host can't access it
        filesystem::mount_rootfs_private()?;
        self.fs.mount()?;
        let rootfs = self.fs.root()?;
        syscall::switch_rootfs(&rootfs)?;
        // Create /dev, /sys, /proc, ...
        filesystem::mount_procfs()?;
        filesystem::mount_sysfs()?;
        filesystem::mount_devfs()?;
        self.event_loop()?;
        log::info!("Container thread stopped");
        Ok(())
    }

    /// Event loop of the container
    fn event_loop(&self) -> Result<()> {
        loop {
            let msg = self.consumer_channel.receive()?;
            log::debug!("Received message: {:?}", msg);
            match msg {
                ipc::Message::ACTION(Action::STOP) => break,
                ipc::Message::COMMAND(command) => {
                    syscall::exec(command)?;
                }
            }
        }
        Ok(())
    }

    /// Get the mountpoint of the container's root filesystem in the host filesystem
    pub fn mount_point(&self) -> Result<&Path> {
        Ok(self.fs.root()?)
    }

}
