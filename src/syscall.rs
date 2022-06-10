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

use std::ffi::CString;
use std::path::Path;
use color_eyre::{Result, eyre};
use nix::mount::{MsFlags, MntFlags, mount, umount2};
use nix::unistd::{pivot_root, chdir, fork, execvpe, ForkResult};
use serde::{Serialize, Deserialize};

/// Switches the current rootfs to `new_root`
/// # Arguments
/// * `new_root` - The path to the new rootfs
pub fn switch_rootfs(new_root: &Path) -> Result<()> {
    mount(
        Some(new_root),
        new_root,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )?;
    // https://man7.org/linux/man-pages/man2/pivot_root.2.html
    // pivot_root(".", ".")
    //  new_root and put_old may be the same directory.  In particular,
    //  the following sequence allows a pivot-root operation without
    //  needing to create and remove a temporary directory:
    //
    //     chdir(new_root);
    //     pivot_root(".", ".");
    //     umount2(".", MNT_DETACH);
    //
    //  This sequence succeeds because the pivot_root() call stacks the
    //  old root mount point on top of the new root mount point at /.  At
    //  that point, the calling process's root directory and current
    //  working directory refer to the new root mount point (new_root).
    //  During the subsequent umount() call, resolution of "."  starts
    //  with new_root and then moves up the list of mounts stacked at /,
    //  with the result that old root mount point is unmounted.
    chdir(new_root)?;
    pivot_root(".", ".")?;
    umount2(".", MntFlags::MNT_DETACH)?;
    Ok(())
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

/// Execute a command
/// # Arguments
/// * `command` - The command to execute
/// # Returns
/// The PID of the new process (only if `exec_type` is `ExecType::FORK`)
/// 
/// Note: when `exec_type` is `ExecType::REPLACE`, this function never returns, as the whole process is replaced.
pub fn exec(command: Command) -> Result<i32> {
    let filename: CString = CString::new(command.command).unwrap();
    let mut args: Vec<CString> = vec![filename.clone()];
    for arg in command.args {
        args.push(CString::new(arg).unwrap());
    }
    let env = &command.env.iter()
        .map(|s| CString::new(s.clone()).unwrap())
        .collect::<Vec<CString>>();
    match command.exec_type {
        ExecType::FORK => {
            // Forking is unsafe ¯\_(ツ)_/¯
            unsafe {
                let fork_result = fork()?;
                match fork_result {
                    ForkResult::Parent { child } => return Ok(i32::from(child)),
                    ForkResult::Child => {
                        execvpe(&filename, &args, &env)?;
                    },
                }
            }
        },
        ExecType::REPLACE => {
            execvpe(&filename, &args, &env)?;
            // On success current process is replaced by the new one
        }
    }
    Err(eyre::eyre!("Failed to execute command"))
}
