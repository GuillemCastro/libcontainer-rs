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

use crate::filesystem::{StorageDriver, NullDriver, self};
use crate::ipc::{self, ConsumerChannel, Action, ProducerChannel};
use crate::syscall::{self, Command, ExecType};
use nix::libc::SIGCHLD;
use nix::sched::{clone, CloneFlags};
use color_eyre::{Result, eyre};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use log;

/// The container struct
pub struct Container {
    /// Root filesystem of the container
    fs: Box<dyn StorageDriver>,
    /// Container's IPC channel
    consumer_channel: ConsumerChannel,
    /// Parent process' IPC channel
    producer_channel: ProducerChannel,
    /// Parent process' PID
    pid: Pid,
    /// Container's PID
    container_pid: Option<Pid>,
}

impl Container {

    const STACK_SIZE: usize = 4 * 1024 * 1024; // == 4 MB

    pub fn default() -> Result<Self> {
        let (producer_channel, consumer_channel) = ipc::create_ipc_channels()?;
        Ok(Container {
            fs: Box::new(NullDriver{}),
            consumer_channel,
            producer_channel,
            pid: Pid::this(),
            container_pid: None,
        })
    }

    pub fn new(fs: Box<dyn StorageDriver>) -> Result<Self> {
        let (producer_channel, consumer_channel) = ipc::create_ipc_channels()?;
        Ok(Container {
            fs,
            consumer_channel,
            producer_channel,
            pid: Pid::this(),
            container_pid: None,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        log::info!("Starting container");
        self.fs.mount()?;

        let ref mut stack: [u8; Container::STACK_SIZE] = [0; Container::STACK_SIZE];
        let flags = Container::clone_flags();
        let callback = Box::new(|| {
            self.container_thread()
        });

        let pid = clone(callback, stack, flags, Some(SIGCHLD))?;
        self.container_pid = Some(pid);
        Ok(())
    }

    /// Forcefully stop the container
    /// Warning: This will immediately kill the container and all its processes, data will be lost
    pub fn force_stop(&mut self) -> Result<()> {
        assert!(self.pid == Pid::this());
        log::info!("Forcefully stopping container");
        self.producer_channel.send(ipc::Message::ACTION(Action::STOP))?;
        // Send a signal to the container thread to stop it IMMEDIATELY
        match &self.container_pid {
            Some(pid) => {
                log::debug!("Sending SIGKILL to container thread");
                kill(*pid, Signal::SIGKILL)?;
            }
            None => {},
        }
        self.unwind()?;
        Ok(())
    }

    /// Wait for the container to finish
    pub fn wait_for_container(&mut self) -> Result<()> {
        // Check we call from the parent process
        assert!(self.pid == Pid::this());
        let pid = match &self.container_pid {
            Some(pid) => *pid,
            None => return Err(eyre::eyre!("Container not started"))
        };
        log::debug!("Waiting for container to finish with PID {}", pid);
        waitpid(pid, None)?;
        self.unwind()?;
        Ok(())
    }

    /// Order the container to execute a new process
    /// # Arguments
    /// * `command` - Filename or path to the executable
    /// * `args` - Arguments to pass to the new process
    /// * `env` - Environment variables to set (optional)
    /// * `exec_type` - Type of execution (optional, default: FORK, see `ExecType`)
    pub fn execute_in_container(&self, command: String, args: Vec<String>, env: Option<Vec<String>>, exec_type: Option<ExecType>) -> Result<()> {
        assert!(self.pid == Pid::this());
        let command = Command {
            command,
            args,
            env: env.unwrap_or(vec![]),
            exec_type: exec_type.unwrap_or(ExecType::REPLACE)
        };
        log::debug!("Executing command inside container {:?}", command);
        self.producer_channel.send(ipc::Message::COMMAND(command))
    }

    fn unwind(&mut self) -> Result<()> {
        self.fs.umount()
    }

    fn container_thread(&mut self) -> isize {
        let rootfs = self.fs.root().unwrap();
        syscall::switch_rootfs(&rootfs).unwrap();
        filesystem::mount_procfs().unwrap();
        filesystem::create_dev_devices().unwrap();
        loop {
            let msg = self.consumer_channel.receive().unwrap();
            log::debug!("Received message: {:?}", msg);
            match msg {
                ipc::Message::ACTION(Action::STOP) => break,
                ipc::Message::COMMAND(command) => {
                    syscall::exec(command).unwrap();
                }
            }
        }
        log::info!("Container thread stopped");
        0
    }

    fn clone_flags() -> CloneFlags {
        CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWIPC | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNET
    }

}