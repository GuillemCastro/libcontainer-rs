/**
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

use crate::filesystem::{Filesystem, NullFilesystem};
use crate::ipc::{self, ConsumerChannel, Action, ProducerChannel, ExecType, Command};
use nix::sched::{clone, CloneFlags};
use color_eyre::{Result};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{Pid, execvpe, fork, ForkResult};
use log;
use std::ffi::{CString};

/// The container struct
pub struct Container {
    /// Root filesystem of the container
    fs: Box<dyn Filesystem>,
    /// Container's IPC channel
    consumer_channel: ConsumerChannel,
    /// Parent process' IPC channel
    producer_channel: ProducerChannel,
    /// Parent process' PID
    pid: Pid,
    /// Container's PID
    container_pid: Option<Pid>,
    container_forked_pids: Vec<Pid>
}

impl Container {

    const STACK_SIZE: usize = 8 * 1024 * 1024; // == 8 MB

    pub fn new() -> Result<Self> {
        let (producer_channel, consumer_channel) = ipc::create_ipc_channels()?;
        Ok(Container {
            fs: Box::new(NullFilesystem{}),
            consumer_channel,
            producer_channel,
            pid: Pid::this(),
            container_pid: None,
            container_forked_pids: Vec::new()
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

        let pid = clone(callback, stack, flags, None)?;
        self.container_pid = Some(pid);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        assert!(self.pid == Pid::this());
        log::info!("Stopping container");
        self.producer_channel.send(ipc::Message::ACTION(Action::STOP))?;
        for pid in &self.container_forked_pids {
            kill(*pid, Signal::SIGTERM)?;
        }
        // Send a signal to the container thread to stop it gracefully
        match &self.container_pid {
            Some(pid) => {
                log::debug!("Sending SIGTERM to container thread");
                kill(*pid, Signal::SIGTERM)?;
            }
            None => {},
        }
        self.fs.umount()?;
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
            exec_type: exec_type.unwrap_or(ExecType::FORK)
        };
        log::debug!("Executing command inside container {:?}", command);
        self.producer_channel.send(ipc::Message::COMMAND(command))
    }

    fn container_thread(&mut self) -> isize {
        loop {
            match self.consumer_channel.receive().unwrap() {
                ipc::Message::ACTION(Action::STOP) => break,
                ipc::Message::COMMAND(command) => self.exec(command).unwrap(),
            }
        }
        log::info!("Container thread stopped");
        0
    }

    fn exec(&mut self, command: Command) -> Result<()> {
        let filename: CString = CString::new(command.command).unwrap();
        let args = &command.args.iter()
            .map(|s| CString::new(s.clone()).unwrap())
            .collect::<Vec<CString>>();
        let env = &command.env.iter()
            .map(|s| CString::new(s.clone()).unwrap())
            .collect::<Vec<CString>>();
        match command.exec_type {
            ExecType::FORK => {
                // Forking is unsafe ¯\_(ツ)_/¯
                unsafe {
                    let fork_result = fork()?;
                    match fork_result {
                        ForkResult::Parent { child } => self.container_forked_pids.push(child),
                        ForkResult::Child => {
                            execvpe(&filename, &args, &env)?;
                            log::error!("Failed to execute command");
                        },
                    }
                }
            },
            ExecType::REPLACE => {
                execvpe(&filename, &args, &env)?;
                // On success current process is replaced by the new one
                log::error!("Failed to execute command");
            }
        }
        Ok(())
    }

    fn clone_flags() -> CloneFlags {
        CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWIPC | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNET | CloneFlags::CLONE_NEWUSER
    }

}