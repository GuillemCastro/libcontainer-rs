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

use color_eyre::eyre::{Result, self};
use nix::mount::{umount2, MntFlags, mount, MsFlags};
use nix::sys::stat::{mknod, SFlag, Mode, makedev};
use std::path::{PathBuf, Path};
use std::{fs, os};
use sys_mount::{Mount, FilesystemType, MountFlags, Unmount, UnmountFlags};

pub trait StorageDriver {

    /// Mounts the filesystem
    fn mount(&mut self) -> Result<()>;

    /// Unmounts the filesystem
    fn umount(&mut self) -> Result<()>;

    /// Returns the path where the filesystem is mounted (inside the parent mount)
    /// e.g. /mnt/my-container/my-fs
    fn root(&self) -> Result<&Path>;

}

pub struct NullDriver {

}

impl StorageDriver for NullDriver {

    /// Mount the filesystem that will be used by the container
    fn mount(&mut self) -> Result<()> {
        todo!()
    }

    /// Unmount the filesystem that was used by the container
    fn umount(&mut self) -> Result<()> {
        todo!()
    }

    /// Return the root path of the filesystem
    fn root(&self) -> Result<&Path> {
        todo!()
    }

}

/// An overlayfs filesystem driver
/// Note: 
pub struct OverlayDriver {
    layers: Vec<PathBuf>,
    target: PathBuf,
    mount: Option<Mount>
}

impl OverlayDriver {

    const MERGE_DIR: &'static str = "merge";
    const UPPER_DIR: &'static str = "upper";
    const WORK_DIR: &'static str = "workdir";

    pub fn new(layers: Vec<&impl AsRef<Path>>, target: &impl AsRef<Path>) -> Self {
        return OverlayDriver {
            layers: layers.iter().map(
                |layer| layer.as_ref().to_path_buf()
            ).collect(),
            target:  target.as_ref().to_path_buf(),
            mount: None
        };
    }

}

impl StorageDriver for OverlayDriver {

    /// Mount an overlayfs that will be used as the filesystem for the container.
    /// 
    /// Overlayfs works by combining several layers of read-only directories (lowerdirs), with a read/write 
    /// directory on top (upperdir). The writes to the resulting filesystem will be saved in the upperdir.
    /// 
    /// This is how the Overlayfs directories will look like
    /// ```
    ///     lowerdirs = the container image
    ///     upperdir = <target>/upper
    ///     workdir = <target>/work
    ///     target = <target>/merge
    /// ```
    /// As a tree,
    /// ```
    ///     <target>/
    ///         upperdir/
    ///         workdir/
    ///         merge/
    /// ```
    /// It is possible to use the current rootfs as the container's image, but as Linux does not allow
    /// to have circular references inside the same filesystem we must put the Overlayfs inside another
    /// filesystem. In this case, a tmpfs. The downside of this election is that the contents of a tmpfs
    /// are stored in memory, and changes are lost when rebooting.
    /// 
    fn mount(&mut self) -> Result<()> {
        if !&self.target.exists() {
            fs::create_dir(&self.target)?;
        }
        // Before mounting, create the Overlay directories
        let mergedir = self.target.join(Self::MERGE_DIR);
        let upperdir = self.target.join(Self::UPPER_DIR);
        let workdir = self.target.join(Self::WORK_DIR);
        if !&mergedir.exists() {
            fs::create_dir(&mergedir)?;
        }
        if !&upperdir.exists() {
            fs::create_dir(&upperdir)?;
        }
        if !&workdir.exists() {
            fs::create_dir(&workdir)?;
        }
        let data = format!("lowerdir={},upperdir={},workdir={}", 
            self.layers.iter().map(
                |layer| layer.display().to_string()
            ).collect::<Vec<String>>().join(":"),  // lowerdir=layer1:layer2:...
            upperdir.display(), // upperdir=upper
            workdir.display() // workdir=work
        );
        let mount = Mount::new(
            "overlay", 
            mergedir,
            FilesystemType::from("overlay"), 
            MountFlags::NOSUID,
            Some(data.as_str())
        )?;
        self.mount = Some(mount);
        Ok(())
    }

    /// Unmount the overlayfs that was used by the container
    fn umount(&mut self) -> Result<()> {
        // If the container is umounted from the host, this will fail.
        if let Some(mount) = self.mount.take() {
            mount.unmount(UnmountFlags::DETACH)?;
        }
        // When calling from the host, instead we have to use the umount2 syscall
        else {
            let mergedir = self.target.join(Self::MERGE_DIR);
            umount2(&mergedir, MntFlags::MNT_DETACH)?;
        }
        Ok(())
    }

    /// Return the root path of the filesystem
    fn root(&self) -> Result<&Path> {
        match self.mount {
            Some(ref mount) => Ok(mount.target_path()),
            None => Err(eyre::eyre!("Filesystem is not mounted"))
        }
    }

}

pub fn mount_rootfs_private() -> Result<()> {
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )?;
    Ok(())
}

pub fn mount_procfs() -> Result<()> {
   Mount::new(
        "proc",
        "/proc",
        FilesystemType::from("proc"),
        MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
        None
    )?;
    Ok(())
}

pub fn mount_sysfs() -> Result<()> {
    Mount::new(
        "tmpfs",
        "/sys",
        FilesystemType::from("tmpfs"),
        MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
        None
    )?;
    Mount::new(
        "sysfs",
        "/sys",
        FilesystemType::from("sysfs"),
        MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
        None
    )?;
    Ok(())
}

pub fn mount_devfs() -> Result<()> {
    Mount::new(
        "tmpfs",
        "/dev",
        FilesystemType::from("tmpfs"),
        MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
        None
    )?;
    create_dev_devices()?;
    Ok(())
}

pub fn create_dev_devices() -> Result<()> {
    // Create some special devices
    mknod("/dev/null", SFlag::S_IFCHR, Mode::S_IRGRP, makedev(1, 3))?;
    mknod("/dev/zero", SFlag::S_IFCHR, Mode::S_IRGRP, makedev(1, 5))?;
    mknod("/dev/full", SFlag::S_IFCHR, Mode::S_IRGRP, makedev(1, 7))?;
    mknod("/dev/random", SFlag::S_IFCHR, Mode::S_IRGRP, makedev(1, 8))?;
    mknod("/dev/urandom", SFlag::S_IFCHR, Mode::S_IRGRP, makedev(1, 9))?;
    mknod("/dev/tty", SFlag::S_IFCHR, Mode::S_IRUSR, makedev(5, 0))?;
    mknod("/dev/console", SFlag::S_IFCHR, Mode::S_IRUSR, makedev(5, 1))?;
    // Create stdin, stdout and stderr
    os::unix::fs::symlink(
        "/proc/self/fd/0",
        "/dev/stdin"
    )?;
    os::unix::fs::symlink(
        "/proc/self/fd/1",
        "/dev/stdout"
    )?;
    os::unix::fs::symlink(
        "/proc/self/fd/2",
        "/dev/stderr"
    )?;
    // Crete /dev/core
    os::unix::fs::symlink(
        "/proc/kcore",
        "/dev/core"
    )?;
    // Create /dev/fd
    os::unix::fs::symlink(
        "/proc/self/fd",
        "/dev/fd"
    )?;
    // Create /dev/mqueue
    fs::create_dir("/dev/mqueue")?;
    Mount::new(
        "mqueue",
        "/dev/mqueue",
        FilesystemType::from("mqueue"),
        MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
        None
    )?;
    // Create /dev/pts
    fs::create_dir("/dev/pts")?;    
    // Mount /dev/pts
    Mount::new(
        "devpts",
        "/dev/pts",
        FilesystemType::from("devpts"),
        MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
        Some("newinstance,ptmxmode=0666,mode=0620")
    )?;
    os::unix::fs::symlink(
        "/dev/pts/ptmx",
        "/dev/ptmx"
    )?;
    // Create /dev/shm
    fs::create_dir("/dev/shm")?;
    // Mount /dev/shm
    Mount::new(
        "shm",
        "/dev/shm",
        FilesystemType::from("tmpfs"),
        MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC,
        Some("mode=1777,size=65536k")
    )?;
    Ok(())
}

mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::fs;
    use std::env;

    #[test]
    fn test_overlay_filesystem_mount() {
        let image = PathBuf::from("/tmp");
        let target = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("tests/test_target");
        let mut fs = OverlayDriver::new(vec![&image], &target);
        fs.mount().unwrap();
        assert!(target.join(OverlayDriver::MERGE_DIR).exists());
        assert!(target.join(OverlayDriver::UPPER_DIR).exists());
        assert!(target.join(OverlayDriver::WORK_DIR).exists());
        fs.umount().unwrap();
        fs::remove_dir_all(target);
    }
}