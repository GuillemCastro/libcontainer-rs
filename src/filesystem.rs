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

use color_eyre::eyre::{Result, self};
use std::path::{PathBuf, Path};
use std::fs;
use sys_mount::{Mount, FilesystemType, MountFlags, Unmount, UnmountFlags};

pub trait Filesystem {

    fn mount(&mut self) -> Result<()>;

    fn umount(&mut self) -> Result<()>;
    
}

pub struct NullFilesystem {

}

impl Filesystem for NullFilesystem {

    /// Mount the filesystem that will be used by the container
    fn mount(&mut self) -> Result<()> {
        todo!()
    }

    /// Unmount the filesystem that was used by the container
    fn umount(&mut self) -> Result<()> {
        todo!()
    }

}

pub struct OverlayFilesystem {
    imagepath: PathBuf,
    targetpath: PathBuf,
    mount: Option<Mount>
}

impl OverlayFilesystem {

    const MERGE_DIR: &'static str = "merge";
    const UPPER_DIR: &'static str = "upper";
    const WORK_DIR: &'static str = "workdir";

    pub fn new(image: &impl AsRef<Path>, target: &impl AsRef<Path>) -> OverlayFilesystem {
        // Overlayfs was introduced in kernel version 3.18
        // match is_kernel_version_compatible("3.18.0") {
        //     Ok(true) => {} // The Kernel version is compatible
        //     _ => { // It might not be compatible, but anyways we can try so just log a warning
        //         warn!("Your kernel version might not be compatible with devenv. Use version 3.18 or greater for better compatibility");
        //     }
        // }
        return OverlayFilesystem {
            imagepath: image.as_ref().to_path_buf(),
            targetpath:  target.as_ref().to_path_buf(),
            mount: None
        };
    }

}

impl Filesystem for OverlayFilesystem {

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
        if self.targetpath.exists() {
            let path = self.targetpath.display();
            return Err(eyre::eyre!("Target path {path} already exists"));
        }
        fs::create_dir(&self.targetpath)?;
        // Before mounting, create the Overlay directories
        fs::create_dir(self.targetpath.join(OverlayFilesystem::MERGE_DIR))?;
        fs::create_dir(self.targetpath.join(OverlayFilesystem::UPPER_DIR))?;
        fs::create_dir(self.targetpath.join(OverlayFilesystem::WORK_DIR))?;
        let data = format!("lowerdir={},upperdir={},workdir={}", 
            self.imagepath.display(),  // lowerdir=image
            self.targetpath.join(OverlayFilesystem::UPPER_DIR).display(), // upperdir=upper
            self.targetpath.join(OverlayFilesystem::WORK_DIR).display() // workdir=work
        );
        let target = self.targetpath.join(OverlayFilesystem::MERGE_DIR); 
        let mount = Mount::new(
            "none", 
            target, 
            FilesystemType::from("overlay"), 
            MountFlags::NOSUID, 
            Some(data.as_str())
        )?;
        self.mount = Some(mount);
        Ok(())
    }

    /// Unmount the overlayfs that was used by the container
    fn umount(&mut self) -> Result<()> {
        if let Some(mount) = self.mount.take() {
            mount.unmount(UnmountFlags::DETACH)?;
        }
        fs::remove_dir_all(&self.targetpath)?;
        Ok(())
    }

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
        let mut fs = OverlayFilesystem::new(&image, &target);
        fs.mount().unwrap();
        assert!(target.join(OverlayFilesystem::MERGE_DIR).exists());
        assert!(target.join(OverlayFilesystem::UPPER_DIR).exists());
        assert!(target.join(OverlayFilesystem::WORK_DIR).exists());
        fs.umount().unwrap();
        fs::remove_dir_all(target);
    }
}