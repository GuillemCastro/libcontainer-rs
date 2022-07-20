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

use std::io::Write;
use std::path::Path;

use crate::filesystem::StorageDriver;
use crate::ipc;
use crate::ipc::Action;
use crate::ipc::ConsumerChannel;
use crate::syscall;
use crate::filesystem;
use crate::syscall::Command;
use crate::syscall::UserInfo;

use color_eyre::Result;
use nix::unistd::sethostname;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeOptions {
    hostname: Option<String>,
    user: String,
    group: String,
    cwd: String,
}

impl RuntimeOptions {
    pub fn default() -> RuntimeOptions {
        RuntimeOptions {
            hostname: None,
            user: "root".to_string(),
            group: "root".to_string(),
            cwd: "/".to_string(),
        }
    }
}

pub struct Runtime {
    // ID of the container
    id: String,
    // Hostname
    hostname: String,
    /// Root filesystem of the container
    fs: Box<dyn StorageDriver>,
    consumer_channel: ConsumerChannel,
    runtime_options: RuntimeOptions
}

impl Runtime {

    /// Create a new container runtime
    /// # Arguments
    /// * `ID` - ID of the container
    /// * `fs` - Root filesystem driver
    /// * `consumer_channel` - Channel for receiving IPC messages
    pub fn new(id: String, fs: Box<dyn StorageDriver>, consumer_channel: ConsumerChannel, runtime_options: RuntimeOptions) -> Runtime {
        let hostname = runtime_options
            .hostname.clone()
            .unwrap_or_else(|| id.clone().chars().take(12).collect());
        Runtime {
            id: id.clone(),
            hostname: hostname,
            fs: fs,
            consumer_channel: consumer_channel,
            runtime_options: runtime_options
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
        self.setup_hostname()?;
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
                    log::debug!("Executing command: {:?}", command);
                    self.exec_command(command)?;
                }
            }
        }
        Ok(())
    }

    /// Get the mountpoint of the container's root filesystem in the host filesystem
    pub fn mount_point(&self) -> Result<&Path> {
        Ok(self.fs.root()?)
    }

    fn exec_command(&self, command: Command) -> Result<()> {
        let environment = self.inject_env_variables(command.env);
        let cmd = Command {
            command: command.command,
            args: command.args,
            env: environment,
            exec_type: command.exec_type,
        };
        syscall::exec(cmd).map(|_| ())
    }

    fn inject_env_variables(&self, environment: Vec<String>) -> Vec<String> {
        let info = UserInfo::from_name(&self.runtime_options.user).unwrap();
        let mut env = environment;
        env.push(format!("{}={}", "container", "libcontainer-rs"));
        env.push(format!("{}={}", "container_uuid", self.id));
        env.push(format!("{}={}", "HOME", info.home));
        env.push(format!("{}={}", "SHELL", info.shell));
        env.push(format!("{}={}", "USER", "root"));
        env.push(format!("{}={}", "HOSTNAME", self.hostname));
        env.push(format!("{}={}", "PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"));
        env
    }

    fn setup_hostname(&self) -> Result<()> {
        // Syscall to set the hostname
        sethostname(self.hostname.as_str())?;
        // Write hostname to /etc/hostname
        let mut hostname_file = std::fs::File::create("/etc/hostname")?;
        hostname_file.write_all(self.hostname.as_bytes())?;
        Ok(())
    }

}
