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

use color_eyre::{Result, eyre};
use ipc_channel::{self, ipc::{IpcSender, IpcReceiver}};
use serde::{Serialize, Deserialize}; 

/// Creates the IPC channel pairs (producer, consumer)
/// # Returns
/// A tuple containing the producer and consumer channels
pub fn create_ipc_channels() -> Result<(ProducerChannel, ConsumerChannel)> {
    let (inner_sender, inner_receiver) = ipc_channel::ipc::channel::<Message>()?;
    Ok((ProducerChannel{inner_sender}, ConsumerChannel{inner_receiver}))
}

/// Execution type for a new process inside the container
#[derive(Debug, Serialize, Deserialize)]
pub enum ExecType {
    /// Execute a new process as a child of the container
    FORK,
    /// Replace the container process with a new one. New process will have PID 0
    REPLACE
}

/// A command represents a process to be executed inside the container
#[derive(Debug, Serialize, Deserialize)]
pub struct Command {
    /// Filename or path to the executable
    pub command: String,
    /// Arguments to pass to the new process
    pub args: Vec<String>,
    /// Environment variables to set
    pub env: Vec<String>,
    /// Execution type for the new process
    pub exec_type: ExecType
}

/// Actions that can be performed by the container
#[derive(Debug, Serialize, Deserialize)]
pub enum Action {
    STOP
}

/// A message to be sent to the container
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    /// Action to be performed by the container
    ACTION(Action),
    /// Command to be executed by the container
    COMMAND(Command)
}

/// The channel to be used by processes outside the container
pub struct ProducerChannel {
    inner_sender: IpcSender<Message>
}

impl ProducerChannel {

    /// Sends a message to the container
    /// # Arguments
    /// * `message` - Message to be sent
    pub fn send(&self, msg: Message) -> Result<()> {
        log::debug!("Sending message: {:?}", msg);
        self.inner_sender.send(msg)?;
        Ok(())
    }
}

/// The channel to be used by processes inside the container
pub struct ConsumerChannel {
    inner_receiver: IpcReceiver<Message>
}

impl ConsumerChannel {

    /// Receives a message for the container
    /// # Returns
    /// The message received
    pub fn receive(&self) -> Result<Message> {
        self.inner_receiver.recv().map_err(|_| eyre::eyre!("Error receiving message"))
    }
}
