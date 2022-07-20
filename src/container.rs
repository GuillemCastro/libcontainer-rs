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

use crate::filesystem::{StorageDriver, NullDriver};
use crate::ipc::{self, Action, ProducerChannel};
use crate::runtime::{Runtime, RuntimeOptions};
use crate::syscall::{self, Command, ExecType};
use crate::random;
use color_eyre::{Result, eyre};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use log;

/// The container struct
pub struct Container {
    /// Parent process' IPC channel
    producer_channel: ProducerChannel,
    /// Parent process' PID
    pid: Pid,
    /// Container's PID
    container_pid: Option<Pid>,
    /// The runtime execution environment for the container
    runtime: Runtime,
}

impl Container {

    pub fn default() -> Result<Self> {
        Ok(
            Container::new(Box::new(NullDriver{}))?
        )
    }

    pub fn new(fs: Box<dyn StorageDriver>) -> Result<Self> {
        let (producer_channel, consumer_channel) = ipc::create_ipc_channels()?;
        let id = random::generate_random_128_id();
        let runtime = Runtime::new(id, fs, consumer_channel, RuntimeOptions::default());
        Ok(Container {
            producer_channel,
            pid: Pid::this(),
            container_pid: None,
            runtime,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        log::info!("Starting container");
        let callback: Box<dyn FnMut() -> isize> = Box::new(|| {
            let res = self.runtime.run();
            if let Err(err) = res {
                log::error!("Container runtime error: {}", err);
                return -1;
            }
            0
        });
        let pid = syscall::create_container(callback)?;
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

}
