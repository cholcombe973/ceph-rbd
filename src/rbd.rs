//! A library to interface with ceph rbd (block device) images.
//!
extern crate ceph;
extern crate log;
extern crate nix;

use self::ceph::ceph::IoCtx;
use self::ceph::error::{RadosError, RadosResult};

use ffi::*;
use get_error;

use std::collections::HashMap;
use std::ffi::CString;
use std::io::{BufRead, Cursor};
use std::marker::PhantomData;
use std::mem::size_of;
use std::os::raw::{c_char, c_void};
use std::ptr;

pub struct Rbd;

#[derive(Debug)]
pub struct RbdImage<'a> {
    image: rbd_image_t,
    phantom: PhantomData<&'a IoCtx>,
}

impl<'a> Drop for RbdImage<'a> {
    fn drop(&mut self) {
        if !self.image.is_null() {
            unsafe {
                let retcode = rbd_close(self.image);
                if retcode < 0 {
                    error!("RbdImage Drop failed");
                }
            }
        }
    }
}

pub struct LockOwner {
    pub name: String,
    pub mode: i32,
}

#[derive(Debug)]
pub struct RbdImageInfo {
    pub size: u64,
    pub obj_size: u64,
    pub num_objs: u64,
    pub order: ::std::os::raw::c_int,
    pub block_name_prefix: String,
    pub parent_pool: i64,
    pub parent_name: Option<String>,
}

impl Rbd {
    /// Create an rbd image
    /// name: what the image is called
    /// size: how big the image is in bytes
    /// order: the image is split into (2**order) byte objects
    /// old_format: whether to create an old-style image that
    /// is accessible by old clients, but can't
    /// use more advanced features like layering.
    /// features: bitmask of features to enable
    /// stripe_unit: stripe unit in bytes (default None to let librbd decide)
    /// stripe_count: objects to stripe over before looping
    /// data_pool: optional separate pool for data blocks
    pub fn create(
        &self,
        ioctx: &IoCtx,
        name: &str,
        size: u64,
        order: Option<u64>,
        features: Option<u64>,
        stripe_unit: Option<u64>,
        stripe_count: Option<u64>,
    ) -> RadosResult<()> {
        let name = CString::new(name)?;
        let stripe_count = match stripe_count {
            Some(c) => c,
            None => 0,
        };
        let stripe_unit = match stripe_unit {
            Some(c) => c,
            None => 0,
        };
        let mut opts: rbd_image_options_t = ptr::null_mut();
        unsafe {
            rbd_image_options_create(&mut opts);
            rbd_image_options_set_uint64(opts, RBD_IMAGE_OPTION_FORMAT as i32, 2);
            if let Some(features) = features {
                let ret_code =
                    rbd_image_options_set_uint64(opts, RBD_IMAGE_OPTION_FEATURES as i32, features);
                if ret_code < 0 {
                    return Err(RadosError::new(get_error(ret_code)?));
                }
            }
            if let Some(order) = order {
                let ret_code =
                    rbd_image_options_set_uint64(opts, RBD_IMAGE_OPTION_ORDER as i32, order);
                if ret_code < 0 {
                    return Err(RadosError::new(get_error(ret_code)?));
                }
            }
            let ret_code = rbd_image_options_set_uint64(
                opts,
                RBD_IMAGE_OPTION_STRIPE_UNIT as i32,
                stripe_unit,
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_image_options_set_uint64(
                opts,
                RBD_IMAGE_OPTION_STRIPE_COUNT as i32,
                stripe_count,
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_create4(*ioctx.inner(), name.as_ptr(), size, opts);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            rbd_image_options_destroy(opts);
        }
        Ok(())
    }

    /// Clone a parent rbd snapshot into a COW sparse child.
    pub fn clone(
        self,
        parent_ioctx: &IoCtx,
        parent_name: &str,
        parent_snapname: &str,
        ioctx: &IoCtx,
        name: &str,
        features: Option<u64>,
        order: Option<u64>,
        stripe_unit: Option<u64>,
        stripe_count: Option<u64>,
    ) -> RadosResult<()> {
        let parent_snapname = CString::new(parent_snapname)?;
        let parent_name = CString::new(parent_name)?;
        let name = CString::new(name)?;
        let stripe_count = match stripe_count {
            Some(c) => c,
            None => 0,
        };
        let stripe_unit = match stripe_unit {
            Some(c) => c,
            None => 0,
        };
        let order = match order {
            Some(c) => c,
            None => 0,
        };
        let mut opts: rbd_image_options_t = ptr::null_mut();

        unsafe {
            rbd_image_options_create(&mut opts);
            if let Some(features) = features {
                let ret_code =
                    rbd_image_options_set_uint64(opts, RBD_IMAGE_OPTION_FEATURES as i32, features);
                if ret_code < 0 {
                    return Err(RadosError::new(get_error(ret_code)?));
                }
            }
            let ret_code = rbd_image_options_set_uint64(opts, RBD_IMAGE_OPTION_ORDER as i32, order);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_image_options_set_uint64(
                opts,
                RBD_IMAGE_OPTION_STRIPE_UNIT as i32,
                stripe_unit,
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_image_options_set_uint64(
                opts,
                RBD_IMAGE_OPTION_STRIPE_COUNT as i32,
                stripe_count,
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_clone3(
                *parent_ioctx.inner(),
                parent_name.as_ptr(),
                parent_snapname.as_ptr(),
                *ioctx.inner(),
                name.as_ptr(),
                opts,
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            rbd_image_options_destroy(opts);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Delete an RBD image. This may take a long time, since it does
    ///not return until every object that comprises the image has
    ///been deleted. Note that all snapshots must be deleted before
    ///the image can be removed.
    pub fn remove(&self, ioctx: &IoCtx, name: &str) -> RadosResult<()> {
        let name = CString::new(name)?;
        unsafe {
            let ret_code = rbd_remove(*ioctx.inner(), name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Rename an RBD image.
    pub fn rename(self, ioctx: &IoCtx, src: &str, dst: &str) -> RadosResult<()> {
        let src = CString::new(src)?;
        let dst = CString::new(dst)?;
        unsafe {
            let ret_code = rbd_rename(*ioctx.inner(), src.as_ptr(), dst.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }
    ///Get pool mirror mode.
    pub fn mirror_mode_get(self, ioctx: &IoCtx) -> RadosResult<rbd_mirror_mode_t> {
        let mut mode = rbd_mirror_mode_t_RBD_MIRROR_MODE_DISABLED;
        unsafe {
            let ret_code = rbd_mirror_mode_get(*ioctx.inner(), &mut mode);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }

        Ok(mode)
    }

    ///Set pool mirror mode.
    pub fn mirror_mode_set(self, ioctx: &IoCtx, mirror_mode: rbd_mirror_mode_t) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_mirror_mode_set(*ioctx.inner(), mirror_mode);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Add mirror peer.
    pub fn mirror_peer_add(
        self,
        ioctx: &IoCtx,
        cluster_name: &str,
        client_name: &str,
    ) -> RadosResult<String> {
        let cluster_name = CString::new(cluster_name)?;
        let client_name = CString::new(client_name)?;

        let mut uuid: Vec<i8> = Vec::with_capacity(512);
        unsafe {
            let ret_code = rbd_mirror_peer_add(
                *ioctx.inner(),
                uuid.as_mut_ptr(),
                uuid.capacity(),
                cluster_name.as_ptr(),
                client_name.as_ptr(),
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        let new_buff: Vec<u8> = uuid.iter().map(|c| c.clone() as u8).collect();
        Ok(String::from_utf8(new_buff)?)
    }

    ///Remove mirror peer.
    pub fn mirror_peer_remove(self, ioctx: &IoCtx, uuid: &str) -> RadosResult<()> {
        let uuid = CString::new(uuid)?;
        unsafe {
            let ret_code = rbd_mirror_peer_remove(*ioctx.inner(), uuid.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Set mirror peer client name
    pub fn mirror_peer_set_client(
        self,
        ioctx: &IoCtx,
        uuid: &str,
        client_name: &str,
    ) -> RadosResult<()> {
        let uuid = CString::new(uuid)?;
        let client_name = CString::new(client_name)?;
        unsafe {
            let ret_code =
                rbd_mirror_peer_set_client(*ioctx.inner(), uuid.as_ptr(), client_name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Set mirror peer cluster name
    pub fn mirror_peer_set_cluster(
        self,
        ioctx: &IoCtx,
        uuid: &str,
        cluster_name: &str,
    ) -> RadosResult<()> {
        let uuid = CString::new(uuid)?;
        let cluster_name = CString::new(cluster_name)?;
        unsafe {
            let ret_code =
                rbd_mirror_peer_set_cluster(*ioctx.inner(), uuid.as_ptr(), cluster_name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Get mirror image status summary of a pool.
    /*
    pub fn mirror_image_status_summary(self, ioctx: &IoCtx) -> RadosResult<()> {
        let mut states = rbd_mirror_image_status_state_t::MIRROR_IMAGE_STATUS_STATE_UNKNOWN;
        let mut counts = 0;
        let mut maxlen = 32;

        unsafe {
            let ret_code =
                rbd_mirror_image_status_summary(ioctx, &mut states, &mut counts, &mut maxlen);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        // TODO: Fix me
        //return [(states[i], counts[i]) for i in range(maxlen)]
        Ok(())
    }
    */

    /*

    pub fn image_set_string(){
    unsafe{
    rbd_image_options_set_string(rbd_image_options_t opts,
                            int optname, const char* optval);
    }
    }

    pub fn image_get_string(){
    unsafe{
    rbd_image_options_get_string(rbd_image_options_t opts,
                            int optname, char* optval,
                            size_t maxlen);
                            }
    }

    pub fn image_get_uint64(){
    unsafe{
    rbd_image_options_get_uint64(rbd_image_options_t opts,
                            int optname, uint64_t* optval);
                            }
    }

    pub fn image_is_set(){
        unsafe{
        rbd_image_options_is_set(rbd_image_options_t opts,
                                                int optname, bool* is_set);
                                            }
    }

    pub fn image_options_unset(){
        unsafe{
        rbd_image_options_unset(rbd_image_options_t opts, int optname);
        }
    }

    pub fn image_options_clear(){
        unsafe{
        rbd_image_options_clear(rbd_image_options_t opts);
        }
    }

    pub fn image_options_is_empty(){
        unsafe{
        rbd_image_options_is_empty(rbd_image_options_t opts);
        }
    }

    */
    pub fn list(&self, ioctx: &IoCtx) -> RadosResult<Vec<String>> {
        let mut name_buff: Vec<i8> = Vec::with_capacity(0);
        let mut name_size: usize = name_buff.capacity();
        // This code below follows the rbd_fuse.cc example in ceph's main repository.
        // If this code is incorrect then so is ceph's.
        // https://github.com/ceph/ceph/blob/jewel/src/rbd_fuse/rbd-fuse.cc#L111
        // This would've been a lot easier to write if ceph had documented how this rbd_list
        // function worked in their header file.
        unsafe {
            trace!("running initial list with size: {}", name_size);
            let retcode = rbd_list(*ioctx.inner(), name_buff.as_mut_ptr(), &mut name_size);
            trace!("rbd_list retcode: {}", retcode);
            // If this is returned that means our buffer was too small
            if retcode == -(nix::errno::Errno::ERANGE as i32) {
                // provided byte array is smaller than listing size
                trace!("Resizing to {}", name_size + 1);
                name_buff = Vec::with_capacity(name_size + 1);
            } else if retcode < 0 {
                // This is an actual error
                return Err(RadosError::new(get_error(retcode)?));
            }

            let retcode = rbd_list(*ioctx.inner(), name_buff.as_mut_ptr(), &mut name_size);

            // And >=0 is how many bytes were written to the list
            if retcode >= 0 {
                // Set the buffer length to the size that ceph wrote to it
                trace!(
                    "retcode: {}. Capacity: {}.  Setting len: {}",
                    retcode,
                    name_buff.capacity(),
                    name_size,
                );
                name_buff.set_len(retcode as usize);
            }
        }

        /*
         * returned byte array contains image names separated by '\0'.
         * the value of rc points to actual size used in the array.
         */
        let mut name_list: Vec<String> = Vec::new();
        let new_buff: Vec<u8> = name_buff.iter().map(|c| c.clone() as u8).collect();
        let mut cursor = Cursor::new(&new_buff);
        loop {
            let mut string_buf: Vec<u8> = Vec::new();
            let read = cursor.read_until(0x00, &mut string_buf)?;
            if read == 0 {
                // End of name_buff;
                break;
            } else {
                // Read a String
                // name_list.push(String::from_utf8_lossy(&string_buf[..read - 1]).into_owned());
                // Remove the trailing \0
                string_buf.pop();
                let s = String::from_utf8(string_buf)?;
                if !s.is_empty() {
                    name_list.push(s);
                }
            }
        }

        return Ok(name_list);
    }

    /// run rbd stat -p {pool} on a given pool IoCtx
    pub fn ceph_rbd_pool_stat(&self, ioctx: &IoCtx) -> RadosResult<rados_pool_stat_t> {    
        unsafe{ 
            let mut pool_stat = ::std::mem::zeroed();
            trace!("running rbd_pool_stat_get");
            let ret_code = rados_ioctx_pool_stat(*ioctx.inner(), &mut pool_stat);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }

            Ok(pool_stat)
        }
    }
    /*

pub fn clone(){
unsafe{
 rbd_clone(&IoCtx p_ioctx, const char *p_name,
	                   const char *p_snapname, &IoCtx c_ioctx,
	                   const char *c_name, uint64_t features, int *c_order);
                       }
 }                       

 pub fn clone2(){
                       unsafe{
 rbd_clone2(&IoCtx p_ioctx, const char *p_name,
	                    const char *p_snapname, &IoCtx c_ioctx,
	                    const char *c_name, uint64_t features, int *c_order,
	                    uint64_t stripe_unit, int stripe_count);
                        }
 }                        

 pub fn clone3(){
                        unsafe{
 rbd_clone3(&IoCtx p_ioctx, const char *p_name,
	                    const char *p_snapname, &IoCtx c_ioctx,
	                    const char *c_name, rbd_image_options_t c_opts);
                        }
 }                        

 pub fn remote(){
    unsafe{
        rbd_remove(&IoCtx io, const char *name);
    }
 } 
 
pub fn remove_with_progress(){
    unsafe{
    rbd_remove_with_progress(&IoCtx io, const char *name,
                                librbd_progress_fn_t cb,
                                            void *cbdata);
                                            }
    }                                          

 pub fn mirror_peer_list(){
                                        unsafe{
 rbd_mirror_peer_list(&IoCtx io_ctx,
                                      rbd_mirror_peer_t *peers, int *max_peers);
                                      }
 }                                      

 pub fn mirror_peer_list_cleanup(){
                                      unsafe{
 rbd_mirror_peer_list_cleanup(rbd_mirror_peer_t *peers,
                                               int max_peers);
                                               }
 }                                               

 pub fn mirror_image_status_list(){
                                             unsafe{
 rbd_mirror_image_status_list(&IoCtx io_ctx,
					      const char *start_id, size_t max,
					      char **image_ids,
					      rbd_mirror_image_status_t *images,
					      size_t *len);
                          }
 }                          

 pub fn mirror_image_status_list_cleanup(){
                          unsafe{
 rbd_mirror_image_status_list_cleanup(char **image_ids,
    rbd_mirror_image_status_t *images, size_t len);
    }
 }    

 pub fn mirror_image_status_summary(){
    unsafe{
 rbd_mirror_image_status_summary(&IoCtx io_ctx,
    rbd_mirror_image_status_state_t *states, int *counts, size_t *maxlen);
    }
 }    
 */
}

impl<'a> RbdImage<'a> {
    /// ioctx can be acquired using the ceph-rust crate
    pub fn open(ioctx: &IoCtx, name: &str, snap_name: &str) -> RadosResult<Self> {
        let name = CString::new(name)?;
        let snap_name = CString::new(snap_name)?;
        let mut rbd: rbd_image_t = ptr::null_mut();

        unsafe {
            let ret_code = rbd_open(*ioctx.inner(), name.as_ptr(), &mut rbd, snap_name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(RbdImage {
            image: rbd,
            phantom: PhantomData,
        })
    }

    /// @desc: closes an rbd image.
    /// Should be called on an RBDImage after a successful open
    pub fn close_image(&self) -> RadosResult<()> {
        if !self.image.is_null() {
            unsafe {
                let retcode = rbd_close(self.image);
                if retcode != 0 {
                    error!("RbdImage close failed with code {}", retcode);
                }
            }
        }
        Ok(())
    }

    /*
pub fn open_by_id(){
                          unsafe{
 rbd_open_by_id(&IoCtx io, const char *id,
                                rbd_image_t *image, const char *snap_name);
                                }
 }                                

pub fn aio_open(){
unsafe{
 rbd_aio_open(&IoCtx io, const char *name,
			      rbd_image_t *image, const char *snap_name,
			      rbd_completion_t c);
                  }
 }                  
pub fn aio_open_by_id(){
                  unsafe{
 rbd_aio_open_by_id(&IoCtx io, const char *id,
                                    rbd_image_t *image, const char *snap_name,
                                    rbd_completion_t c);
                                    }
 }                                    
 */
    /// This is intended for use by clients that cannot write to a block
    /// device due to cephx restrictions. There will be no watch
    /// established on the header object, since a watch is a write. This
    /// means the metadata reported about this image (parents, snapshots,
    /// size, etc.) may become stale. This should not be used for
    /// long-running operations, unless you can be sure that one of these
    /// properties changing is safe.
    /// Attempting to write to a read-only image will return -EROFS.
    /// Open an image in read-only mode.
    /// ioctx can be acquired using the ceph-rust crate
    pub fn open_read_only(ioctx: &IoCtx, name: &str, snap_name: &str) -> RadosResult<Self> {
        let name = CString::new(name)?;
        let snap_name = CString::new(snap_name)?;
        let mut rbd: rbd_image_t = ptr::null_mut();

        unsafe {
            let ret_code =
                rbd_open_read_only(*ioctx.inner(), name.as_ptr(), &mut rbd, snap_name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(RbdImage {
            image: rbd,
            phantom: PhantomData,
        })
    }

    /**
pub fn open_by_id_read_only(){
                                    unsafe{
 rbd_open_by_id_read_only(&IoCtx io, const char *id,
                                          rbd_image_t *image, const char *snap_name);
                                          }
 }                                          

pub fn aio_open_read_open(){
                                          unsafe{
 rbd_aio_open_read_only(&IoCtx io, const char *name,
					rbd_image_t *image, const char *snap_name,
					rbd_completion_t c);
                    }
 }                    

pub fn aio_open_by_id_read_open(){
                    unsafe{
 rbd_aio_open_by_id_read_only(&IoCtx io, const char *id,
                                              rbd_image_t *image, const char *snap_name,
                                              rbd_completion_t c);
                                              }
 }                                              

pub fn aio_close(){
 unsafe{
 rbd_aio_close(rbd_image_t image, rbd_completion_t c);
 }
 } 

pub fn resize2(){
 unsafe{
 rbd_resize2(rbd_image_t image, uint64_t size, bool allow_shrink,
			     librbd_progress_fn_t cb, void *cbdata);
                 }
 }                 

 pub fn resize_with_progress(){
                 unsafe{
 rbd_resize_with_progress(rbd_image_t image, uint64_t size,
			     librbd_progress_fn_t cb, void *cbdata);
                 }
 }                 
 */
///Change the size of the image.
    pub fn resize(&self, size: u64) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_resize(self.image, size);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    pub fn stat(&self) -> RadosResult<RbdImageInfo> {
        let mut info = rbd_image_info_t {
            size: 0,
            obj_size: 0,
            num_objs: 0,
            order: 0,
            block_name_prefix: [0; 24],
            parent_pool: 0,
            parent_name: [0; 96],
        };
        unsafe {
            let ret_code = rbd_stat(self.image, &mut info, size_of::<rbd_image_info_t>());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        let name_prefix_vec: Vec<u8> = info
            .block_name_prefix
            .iter()
            .map(|c| c.clone() as u8)
            .filter(|c| c > &0)
            .collect();
        let name_prefix = String::from_utf8(name_prefix_vec)?;
        let parent_name_vec: Vec<u8> = info
            .parent_name
            .iter()
            .map(|c| c.clone() as u8)
            .filter(|c| c > &0)
            .collect();
        let parent_name = {
            let s = String::from_utf8(parent_name_vec)?;
            match s.is_empty() {
                true => None,
                false => Some(s),
            }
        };

        Ok(RbdImageInfo {
            size: info.size,
            obj_size: info.obj_size,
            num_objs: info.num_objs,
            order: info.order,
            block_name_prefix: name_prefix,
            parent_pool: info.parent_pool,
            parent_name: parent_name,
        })
    }

    pub fn get_size(&self) -> RadosResult<u64> {
        let mut size: u64 = 0;
        unsafe {
            let ret_code = rbd_get_size(self.image, &mut size);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(size)
    }

    ///Get the RBD v2 internal image id
    pub fn id(&self) -> RadosResult<String> {
        let mut image_id: Vec<i8> = Vec::with_capacity(4096);
        let capacity = image_id.capacity();

        unsafe {
            let ret_code = rbd_get_id(self.image, image_id.as_mut_ptr(), image_id.capacity());
            if ret_code == -(nix::errno::Errno::ERANGE as i32) {
                image_id.reserve(capacity);
            }
            // Resize buffer to 2x and try again
            let ret_code = rbd_get_id(self.image, image_id.as_mut_ptr(), image_id.capacity());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        let s: Vec<u8> = image_id.iter().map(|c| c.clone() as u8).collect();
        Ok(String::from_utf8(s)?)
    }

    pub fn pool_id(&self) -> i64 {
        unsafe {
            let pool_id = rbd_get_data_pool_id(self.image);
            return pool_id;
        }
    }

    ///Get the RBD block name prefix
    pub fn block_name_prefix(&self) -> RadosResult<String> {
        let mut prefix: Vec<i8> = Vec::with_capacity(4096);
        let capacity = prefix.capacity();

        unsafe {
            let ret_code = rbd_get_id(self.image, prefix.as_mut_ptr(), prefix.capacity());
            if ret_code == -(nix::errno::Errno::ERANGE as i32) {
                prefix.reserve(capacity);
            }
            // Resize buffer to 2x and try again
            let ret_code =
                rbd_get_block_name_prefix(self.image, prefix.as_mut_ptr(), prefix.capacity());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        let s: Vec<u8> = prefix.iter().map(|c| c.clone() as u8).collect();
        Ok(String::from_utf8(s)?)
    }

    ///Gets the features bitmask of the image.
    pub fn features(&self) -> RadosResult<u64> {
        let mut features: u64 = 0;
        unsafe {
            let ret_code = rbd_get_features(self.image, &mut features);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(features)
    }

    /// Updates the features bitmask of the image by enabling/disabling
    /// a single feature.  The feature must support the ability to be
    /// dynamically enabled/disabled.
    pub fn update_features(&self, features: u64, enabled: bool) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_update_features(self.image, features, enabled as u8);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Gets the number of overlapping bytes between the image and its parent
    /// image. If open to a snapshot, returns the overlap between the snapshot
    /// and the parent image.
    pub fn overlap(&self) -> RadosResult<u64> {
        let mut overlap: u64 = 0;
        unsafe {
            let ret_code = rbd_get_overlap(self.image, &mut overlap);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(overlap)
    }

    /// Gets the flags bitmask of the image.
    pub fn flags(&self) -> RadosResult<u64> {
        let mut flags: u64 = 0;
        unsafe {
            let ret_code = rbd_get_flags(self.image, &mut flags);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(flags)
    }

    /// Gets the status of the image exclusive lock.
    pub fn is_exclusive_lock_owner(&self) -> RadosResult<bool> {
        let mut owner: i32 = 0;
        unsafe {
            let ret_code = rbd_is_exclusive_lock_owner(self.image, &mut owner);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(owner == 1)
    }
    /// Copy the image to another location.
    pub fn copy(
        &self,
        dest_ioctx: &IoCtx,
        dest_name: &str,
        features: Option<u64>,
        order: Option<u64>,
        stripe_unit: Option<u64>,
        stripe_count: Option<u64>,
    ) -> RadosResult<()> {
        let order = match order {
            Some(o) => o,
            None => 0,
        };
        let stripe_count = match stripe_count {
            Some(s) => s,
            None => 0,
        };
        let stripe_unit = match stripe_unit {
            Some(s) => s,
            None => 0,
        };
        let dest_name = CString::new(dest_name)?;
        let mut opts: rbd_image_options_t = ptr::null_mut();

        unsafe {
            rbd_image_options_create(&mut opts);

            if let Some(features) = features {
                let ret_code =
                    rbd_image_options_set_uint64(opts, RBD_IMAGE_OPTION_FEATURES as i32, features);
                if ret_code < 0 {
                    return Err(RadosError::new(get_error(ret_code)?));
                }
            }
            let ret_code = rbd_image_options_set_uint64(opts, RBD_IMAGE_OPTION_ORDER as i32, order);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_image_options_set_uint64(
                opts,
                RBD_IMAGE_OPTION_STRIPE_UNIT as i32,
                stripe_unit,
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_image_options_set_uint64(
                opts,
                RBD_IMAGE_OPTION_STRIPE_COUNT as i32,
                stripe_count,
            );
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            let ret_code = rbd_copy3(self.image, *dest_ioctx.inner(), dest_name.as_ptr(), opts);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
            rbd_image_options_destroy(opts);
        }
        Ok(())
    }

    /// Create a snapshot of the image.
    pub fn create_snap(&self, name: &str) -> RadosResult<()> {
        let name = CString::new(name)?;
        unsafe {
            let ret_code = rbd_snap_create(self.image, name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// rename a snapshot of the image.
    pub fn rename_snap(&self, srcname: &str, dstname: &str) -> RadosResult<()> {
        let srcname = CString::new(srcname)?;
        let dstname = CString::new(dstname)?;
        unsafe {
            let ret_code = rbd_snap_rename(self.image, srcname.as_ptr(), dstname.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Delete a snapshot of the image.
    pub fn remove_snap(&self, name: &str) -> RadosResult<()> {
        let name = CString::new(name)?;
        unsafe {
            let ret_code = rbd_snap_remove(self.image, name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Revert the image to its contents at a snapshot. This is a
    ///potentially expensive operation, since it rolls back each
    ///object individually.
    pub fn rollback_to_snap(&self, name: &str) -> RadosResult<()> {
        let name = CString::new(name)?;
        unsafe {
            let ret_code = rbd_snap_rollback(self.image, name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Mark a snapshot as protected. This means it can't be deleted
    ///until it is unprotected.
    pub fn protect_snap(&self, name: &str) -> RadosResult<()> {
        let name = CString::new(name)?;
        unsafe {
            let ret_code = rbd_snap_protect(self.image, name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Mark a snapshot unprotected. This allows it to be deleted if
    ///it was protected.
    pub fn unprotect_snap(&self, name: &str) -> RadosResult<()> {
        let name = CString::new(name)?;
        unsafe {
            let ret_code = rbd_snap_unprotect(self.image, name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Find out whether a snapshot is protected from deletion.
    pub fn is_protected_snap(&self, name: &str) -> RadosResult<bool> {
        let name = CString::new(name)?;
        let mut is_protected: i32 = 0;
        unsafe {
            let ret_code = rbd_snap_is_protected(self.image, name.as_ptr(), &mut is_protected);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(is_protected == 1)
    }

    ///Set the snapshot to read from. Writes will raise ReadOnlyImage
    ///while a snapshot is set. Pass None to unset the snapshot
    ///(reads come from the current image) , and allow writing again.
    pub fn set_snap(&self, name: &str) -> RadosResult<()> {
        let name = CString::new(name)?;
        unsafe {
            let ret_code = rbd_snap_set(self.image, name.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Trim the range from the image. It will be logically filled
    ///with zeroes.
    pub fn discard(&self, offset: u64, length: u64) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_discard(self.image, offset, length);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Block until all writes are fully flushed if caching is enabled.
    pub fn flush(&self) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_flush(self.image);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    ///Drop any cached data for the image.
    pub fn invalidate_cache(&self) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_invalidate_cache(self.image);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Returns the stripe unit used for the image.
    pub fn stripe_unit(&self) -> RadosResult<u64> {
        let mut stripe_unit: u64 = 0;
        unsafe {
            let ret_code = rbd_get_stripe_unit(self.image, &mut stripe_unit);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(stripe_unit)
    }

    /// Returns the stripe count used for the image.
    pub fn stripe_count(&self) -> RadosResult<u64> {
        let mut stripe_count: u64 = 0;
        unsafe {
            let ret_code = rbd_get_stripe_count(self.image, &mut stripe_count);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(stripe_count)
    }

    /// Flatten clone image (copy all blocks from parent to child)
    pub fn flatten(&self) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_flatten(self.image);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /*
    /// Rebuilds the object map for the image HEAD or currently set snapshot
    pub fn rebuild_object_map(&self)->RadosResult<()>{
        // librbd_progress_fn_t prog_cb = &no_op_progress_callback
        unsafe{
            let ret_code = rbd_rebuild_object_map(self.image, prog_cb, NULL);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }
    */

    /// Acquire a managed lock on the image.
    pub fn lock_acquire(&self, lock_mode: rbd_lock_mode_t) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_lock_acquire(self.image, lock_mode);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    // Release a managed lock on the image that was previously acquired.
    pub fn lock_release(&self) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_lock_release(self.image);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    // Break the image lock held by a another client.
    pub fn lock_break(&self, lock_mode: rbd_lock_mode_t, lock_owner: &str) -> RadosResult<()> {
        let lock_owner = CString::new(lock_owner)?;
        unsafe {
            let ret_code = rbd_lock_break(self.image, lock_mode, lock_owner.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Take an exclusive lock on the image.
    pub fn lock_exclusive(&self, cookie: &str) -> RadosResult<()> {
        let cookie = CString::new(cookie)?;
        unsafe {
            let ret_code = rbd_lock_exclusive(self.image, cookie.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Take a shared lock on the image. The tag must match
    /// that of the existing lockers, if any.
    pub fn lock_shared(&self, cookie: &str, tag: &str) -> RadosResult<()> {
        let cookie = CString::new(cookie)?;
        let tag = CString::new(tag)?;
        unsafe {
            let ret_code = rbd_lock_shared(self.image, cookie.as_ptr(), tag.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Release a lock on the image that was locked by this rados client.
    pub fn unlock(&self, cookie: &str) -> RadosResult<()> {
        let cookie = CString::new(cookie)?;
        unsafe {
            let ret_code = rbd_unlock(self.image, cookie.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Release a lock held by another rados client.
    pub fn break_lock(&self, client: &str, cookie: &str) -> RadosResult<()> {
        let client = CString::new(client)?;
        let cookie = CString::new(cookie)?;
        unsafe {
            let ret_code = rbd_break_lock(self.image, client.as_ptr(), cookie.as_ptr());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Enable mirroring for the image.
    pub fn mirror_image_enable(&self) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_mirror_image_enable(self.image);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Disable mirroring for the image.
    pub fn mirror_image_disable(&self, force: bool) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_mirror_image_disable(self.image, force);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Promote the image to primary for mirroring.
    pub fn mirror_image_promote(&self, force: bool) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_mirror_image_promote(self.image, force);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Demote the image to secondary for mirroring.
    pub fn mirror_image_demote(&self) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_mirror_image_demote(self.image);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /// Flag the image to resync.
    pub fn mirror_image_resync(&self) -> RadosResult<()> {
        unsafe {
            let ret_code = rbd_mirror_image_resync(self.image);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /*
    /// Get mirror info for the image.
     pub fn mirror_image_get_info(&self)->RadosResult<RbdMirrorImageInfo>{
         let mut global_id_buffer: Vec<i8> = Vec::with_capacity(4096);
        let mut info = rbd_mirror_image_info_t{
            global_id: global_id_buffer,
            state: rbd_mirror_image_state_t::RBD_MIRROR_IMAGE_DISABLING,
            primary: false,
        };
        unsafe{
            let ret_code = rbd_mirror_image_get_info(self.image, 
                &mut info, 
                ::std::mem::sizeof::<rbd_mirror_image_info_t>());
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        let rbd_info = RbdMirrorImageInfo{
            global_id : info.global_id,
            state     : info.state,
            primary   : info.primary,
        };

        Ok(rbd_info)
     }

    pub fn mirror_image_get_status(&self)->RadosResult<()>{
        """
        Get mirror status for the image.
        :returns: dict - contains the following keys:
            * ``name`` (str) - mirror image name
            * `info` (dict) - mirror image info
            * ``state`` (int) - status mirror state
            * ``description`` (str) - status description
            * ``last_update`` (datetime) - last status update time
            * ``up`` (bool) - is mirroring agent up
        """
        cdef rbd_mirror_image_status_t c_status
        with nogil:
            ret = rbd_mirror_image_get_status(self.image, &c_status,
                                              sizeof(c_status))
        if ret != 0:
            raise make_ex(ret, 'error getting mirror status for image %s' %
                          (self.name,))
        status = {
            'name'      : decode_cstr(c_status.name),
            'info'      : {
                'global_id' : decode_cstr(c_status.info.global_id),
                'state'     : int(c_status.info.state),
                'primary'   : c_status.info.primary,
                },
            'state'       : c_status.state,
            'description' : decode_cstr(c_status.description),
            'last_update' : datetime.fromtimestamp(c_status.last_update),
            'up'          : c_status.up,
            }
        free(c_status.name)
        free(c_status.info.global_id)
        free(c_status.description)
    return status
    }
     */

    /*

pub fn get_parent_info(){
unsafe{
 rbd_get_parent_info(rbd_image_t image,
			             char *parent_poolname, size_t ppoolnamelen,
			             char *parent_name, size_t pnamelen,
			             char *parent_snapname,
                                     size_t psnapnamelen);
                                      }
 }

pub fn get_parent_info2(){
                                     unsafe{
 rbd_get_parent_info2(rbd_image_t image,
                                      char *parent_poolname,
                                      size_t ppoolnamelen,
                                      char *parent_name, size_t pnamelen,
                                      char *parent_id, size_t pidlen,
                                      char *parent_snapname,
                                      size_t psnapnamelen);

pub fn get_group(){
 unsafe{
 rbd_get_group(rbd_image_t image, rbd_group_info_t *group_info,
                               size_t group_info_size);
                                }
 }

pub fn set_image_notification(){
                               unsafe{
 rbd_set_image_notification(rbd_image_t image, int fd, int type);
  }
 }
 */

    pub fn get_lock_owners(&self) -> RadosResult<()> {
        let mut lock_mode = rbd_lock_mode_t_RBD_LOCK_MODE_SHARED;
        let mut max_owners: usize = 8;

        let mut owners = vec![ptr::null_mut::<::std::os::raw::c_char>(); max_owners];
        loop {
            unsafe {
                let ret_code = rbd_lock_get_owners(
                    self.image,
                    &mut lock_mode,
                    owners.as_mut_ptr(),
                    &mut max_owners,
                );
                println!("lock owners ret_code: {}", ret_code);
                if ret_code >= 0 {
                    //println!("owners: {:?}", owners);
                    break;
                } else if ret_code == -(nix::errno::Errno::ENOENT as i32) {
                    break;
                } else if ret_code != -(nix::errno::Errno::ERANGE as i32) {
                    return Err(RadosError::new(get_error(ret_code)?));
                }
            }
        }
        unsafe {
            rbd_lock_get_owners_cleanup(owners.as_mut_ptr(), max_owners);
        }
        Ok(())
    }
    /*
pub fn lock_get_owners_cleanup(){
                                     unsafe{
 rbd_lock_get_owners_cleanup(char **lock_owners,
                                              size_t lock_owner_count);
                                               }
 }

 pub fn copy_with_progress(){
 unsafe{
 rbd_copy_with_progress(rbd_image_t image, &IoCtx dest_p,
                                        const char *destname,
                                        librbd_progress_fn_t cb, void *cbdata);
                                         }
 }

 pub fn copy_with_progress2(){
 unsafe{
 rbd_copy_with_progress2(rbd_image_t src, rbd_image_t dest,
			                 librbd_progress_fn_t cb, void *cbdata);
                              }
 }

 pub fn copy_with_progress3(){
 unsafe{
 rbd_copy_with_progress3(rbd_image_t image,
					 &IoCtx dest_p,
					 const char *destname,
					 rbd_image_options_t dest_opts,
					 librbd_progress_fn_t cb, void *cbdata);
                      }
 }

 pub fn copy_with_progress4(){
 unsafe{
 rbd_copy_with_progress4(rbd_image_t image,
					 &IoCtx dest_p,
					 const char *destname,
					 rbd_image_options_t dest_opts,
					 librbd_progress_fn_t cb, void *cbdata,
					 size_t sparse_size);
                      }
 }

/* deep copy */
pub fn deep_copy(){
 unsafe{
 rbd_deep_copy(rbd_image_t src, &IoCtx dest_io_ctx,
                               const char *destname,
                               rbd_image_options_t dest_opts);
                                }
 }

pub fn deep_copy_with_progress(){
 unsafe{
 rbd_deep_copy_with_progress(rbd_image_t image,
                                             &IoCtx dest_io_ctx,
                                             const char *destname,
                                             rbd_image_options_t dest_opts,
                                             librbd_progress_fn_t cb,
                                             void *cbdata);
                                              }
 }

/* snapshots */
pub fn snap_list(){
 unsafe{
 rbd_snap_list(rbd_image_t image, rbd_snap_info_t *snaps,
                               int *max_snaps);
                                }
 }
pub fn snap_list_end(){
 unsafe{
 rbd_snap_list_end(rbd_snap_info_t *snaps);
  }
 }


pub fn snap_remove2(){
 unsafe{
 rbd_snap_remove2(rbd_image_t image, const char *snap_name, uint32_t flags,
				  librbd_progress_fn_t cb, void *cbdata);
                   }
 }


pub fn snap_rollback_with_progress(){
 unsafe{
 rbd_snap_rollback_with_progress(rbd_image_t image,
                                                 const char *snapname,
				                 librbd_progress_fn_t cb,
                                                 void *cbdata);
                                                  }
 }

///Get the current snapshot limit for an image. If no limit is set,
///UINT64_MAX is returned.
///@param limit pointer where the limit will be stored on success
///@returns 0 on success, negative error code on failure
pub fn snap_get_limit(){
 unsafe{
 rbd_snap_get_limit(rbd_image_t image, uint64_t *limit);
  }
}

///Set a limit for the number of snapshots that may be taken of an image.
///@param limit the maximum number of snapshots allowed in the future.
///@returns 0 on success, negative error code on failure
pub fn snap_set_limit(){
 unsafe{
 rbd_snap_set_limit(rbd_image_t image, uint64_t limit);
  }
}

///Get the timestamp of a snapshot for an image. 
///@param snap_id the snap id of a snapshot of input image.
///@param timestamp the timestamp of input snapshot.
///@returns 0 on success, negative error code on failure
pub fn snap_get_timestamp(){
 unsafe{
 rbd_snap_get_timestamp(rbd_image_t image, uint64_t snap_id, struct timespec *timestamp);
  }
}

pub fn snap_get_namespace_type(){
 unsafe{
 rbd_snap_get_namespace_type(rbd_image_t image,
					     uint64_t snap_id,
					     rbd_snap_namespace_type_t *namespace_type);
                          }
}

pub fn snap_get_group_namespace(){
 unsafe{
 rbd_snap_get_group_namespace(rbd_image_t image,
                                              uint64_t snap_id,
                                              rbd_snap_group_namespace_t *group_snap,
                                              size_t group_snap_size);
                                               }
}

pub fn snap_group_namespace_cleanup(){
 unsafe{
 rbd_snap_group_namespace_cleanup(rbd_snap_group_namespace_t *group_snap,
                                                  size_t group_snap_size);
                                                   }
}

pub fn flatten_with_progress(){
 unsafe{
 rbd_flatten_with_progress(rbd_image_t image,
                                           librbd_progress_fn_t cb,
                                           void *cbdata);
                                            }
}

///List all images that are cloned from the image at the
///snapshot that is set via rbd_snap_set().
///This iterates over all pools, so it should be run by a user with
///read access to all of them. pools_len and images_len are filled in
///with the number of bytes put into the pools and images buffers.
///If the provided buffers are too short, the required lengths are
///still filled in, but the data is not and -ERANGE is returned.
///Otherwise, the buffers are filled with the pool and image names
///of the children, with a '\0' after each.
///@param image which image (and implicitly snapshot) to list clones of
///@param pools buffer in which to store pool names
///@param pools_len number of bytes in pools buffer
///@param images buffer in which to store image names
///@param images_len number of bytes in images buffer
///@returns number of children on success, negative error code on failure
///@returns -ERANGE if either buffer is too short
pub fn list_children(){
 unsafe{
 ssize_t rbd_list_children(rbd_image_t image, char *pools,
                                       size_t *pools_len, char *images,
                                       size_t *images_len);
                                        }
}

pub fn list_children2(){
 unsafe{
 rbd_list_children2(rbd_image_t image,
                                    rbd_child_info_t *children,
                                    int *max_children);
                                     }
}
pub fn list_child_cleanup(){
 unsafe{
 rbd_list_child_cleanup(rbd_child_info_t *child);
  }
}

pub fn list_children_cleanup(){
 unsafe{
 rbd_list_children_cleanup(rbd_child_info_t *children,
                                            size_t num_children);
 }
                                             }

///@defgroup librbd_h_locking Advisory Locking
///An rbd image may be locking exclusively, or shared, to facilitate
///e.g. live migration where the image may be open in two places at once.
///These locks are intended to guard against more than one client
///writing to an image without coordination. They don't need to
///be used for snapshots, since snapshots are read-only.
///Currently locks only guard against locks being acquired.
///They do not prevent anything else.
///A locker is identified by the internal rados client id of the
///holder and a user-defined cookie. This (client id, cookie) pair
///must be unique for each locker.
///A shared lock also has a user-defined tag associated with it. Each
///additional shared lock must specify the same tag or lock
///acquisition will fail. This can be used by e.g. groups of hosts
///using a clustered filesystem on top of an rbd image to make sure
///they're accessing the correct image.

///@{
///List clients that have locked the image and information about the lock.
///The number of bytes required in each buffer is put in the
///corresponding size out parameter. If any of the provided buffers
///are too short, -ERANGE is returned after these sizes are filled in.
///@param exclusive where to store whether the lock is exclusive (1) or shared (0)
///@param tag where to store the tag associated with the image
///@param tag_len number of bytes in tag buffer
///@param clients buffer in which locker clients are stored, separated by '\0'
///@param clients_len number of bytes in the clients buffer
///@param cookies buffer in which locker cookies are stored, separated by '\0'
///@param cookies_len number of bytes in the cookies buffer
///@param addrs buffer in which locker addresses are stored, separated by '\0'
///@param addrs_len number of bytes in the clients buffer
///@returns number of lockers on success, negative error code on failure
///@returns -ERANGE if any of the buffers are too short
pub fn list_lockers(){
 unsafe{
 ssize_t rbd_list_lockers(rbd_image_t image, int *exclusive,
			              char *tag, size_t *tag_len,
			              char *clients, size_t *clients_len,
			              char *cookies, size_t *cookies_len,
			              char *addrs, size_t *addrs_len);
                           }
}
*/

    /* I/O */
    /// If the given buffer is too small this will reserve additional capacity from it
    pub fn read(&self, buffer: &mut Vec<i8>, offset: u64, length: usize) -> RadosResult<isize> {
        let buf_size = buffer.capacity();
        if length > buf_size {
            buffer.reserve(length - buf_size);
        }
        unsafe {
            let size_read = rbd_read(self.image, offset, length, buffer.as_mut_ptr());
            if size_read < 0 {
                return Err(RadosError::new(get_error(size_read as i32)?));
            }
            buffer.set_len(size_read as usize);
            Ok(size_read)
        }
    }
    /*

///@param op_flags: see librados.h constants beginning with LIBRADOS_OP_FLAG
pub fn read(){
 unsafe{
 ssize_t rbd_read2(rbd_image_t image, uint64_t ofs, size_t len,
                               char *buf, int op_flags);
                                }
}
*/

    ///Iterate read over an image
    ///Reads each region of the image and calls the callback.  If the
    ///buffer pointer passed to the callback is NULL, the given extent is
    ///defined to be zeros (a hole).  Normally the granularity for the
    ///callback is the image stripe size.
    ///@param image image to read
    ///@param ofs offset to start from
    ///@param len bytes of source image to cover
    ///@param cb callback for each region
    ///@returns 0 success, error otherwise
    pub fn read_iterate2(
        &self,
        offset: u64,
        length: u64,
        callback: Option<unsafe extern "C" fn(u64, usize, *const c_char, *mut c_void) -> i32>,
    ) -> RadosResult<()> {
        let p: *mut c_void = ptr::null_mut();
        unsafe {
            let ret_code = rbd_read_iterate2(self.image, offset, length, callback, p);
            if ret_code < 0 {
                return Err(RadosError::new(get_error(ret_code)?));
            }
        }
        Ok(())
    }

    /*
///get difference between two versions of an image
///This will return the differences between two versions of an image
///via a callback, which gets the offset and length and a flag
///indicating whether the extent exists (1), or is known/defined to
///be zeros (a hole, 0).  If the source snapshot name is NULL, we
///interpret that as the beginning of time and return all allocated
///regions of the image.  The end version is whatever is currently
///selected for the image handle (either a snapshot or the writeable
///head).
///@param fromsnapname start snapshot name, or NULL
///@param ofs start offset
///@param len len in bytes of region to report on
///@param include_parent 1 if full history diff should include parent
///@param whole_object 1 if diff extents should cover whole object
///@param cb callback to call for each allocated region
///@param arg argument to pass to the callback
///@returns 0 on success, or negative error code on error
pub fn diff_iterate(){
 unsafe{
 rbd_diff_iterate(rbd_image_t image,
		                  const char *fromsnapname,
		                  uint64_t ofs, uint64_t len,
		                  int (*cb)(uint64_t, size_t, int, void *),
                                  void *arg);
 }
                                   }
pub fn diff_iterate2(){
 unsafe{
 rbd_diff_iterate2(rbd_image_t image,
		                   const char *fromsnapname,
		                   uint64_t ofs, uint64_t len,
                                   uint8_t include_parent, uint8_t whole_object,
		                   int (*cb)(uint64_t, size_t, int, void *),
                                   void *arg);
                                    }
                                    }
pub fn write(){
 unsafe{
 ssize_t rbd_write(rbd_image_t image, uint64_t ofs, size_t len,
                               const char *buf);
                                }
                                    }
///@param op_flags: see librados.h constants beginning with LIBRADOS_OP_FLAG
pub fn write2(){
 unsafe{
 ssize_t rbd_write2(rbd_image_t image, uint64_t ofs, size_t len,
                                const char *buf, int op_flags);
                                 }
                                    }

pub fn writesame(){
 unsafe{
 ssize_t rbd_writesame(rbd_image_t image, uint64_t ofs, size_t len,
                                   const char *buf, size_t data_len, int op_flags);
                                    }
                                    }

pub fn compare_and_write(){
 unsafe{
 ssize_t rbd_compare_and_write(rbd_image_t image, uint64_t ofs,
                                           size_t len, const char *cmp_buf,
                                           const char *buf, uint64_t *mismatch_off,
                                           int op_flags);
                                            }
                                    }

pub fn aio_write(){
 unsafe{
 rbd_aio_write(rbd_image_t image, uint64_t off, size_t len,
                               const char *buf, rbd_completion_t c);
                                }
                                    }

///@param op_flags: see librados.h constants beginning with LIBRADOS_OP_FLAG
pub fn aio_write2(){
 unsafe{
 rbd_aio_write2(rbd_image_t image, uint64_t off, size_t len,
                                const char *buf, rbd_completion_t c,
                                int op_flags);
                                 }
                                    }

pub fn aio_writev(){
 unsafe{
 rbd_aio_writev(rbd_image_t image, const struct iovec *iov,
                                int iovcnt, uint64_t off, rbd_completion_t c);
                                 }
}

pub fn aio_read(){
 unsafe{
 rbd_aio_read(rbd_image_t image, uint64_t off, size_t len,
                              char *buf, rbd_completion_t c);
                               }
}

///@param op_flags: see librados.h constants beginning with LIBRADOS_OP_FLAG
pub fn aio_read2(){
 unsafe{
 rbd_aio_read2(rbd_image_t image, uint64_t off, size_t len,
                               char *buf, rbd_completion_t c, int op_flags);
                                }
}

pub fn aio_readv(){
 unsafe{
 rbd_aio_readv(rbd_image_t image, const struct iovec *iov,
                               int iovcnt, uint64_t off, rbd_completion_t c);
                                }
}

pub fn aio_discard(){
 unsafe{
 rbd_aio_discard(rbd_image_t image, uint64_t off, uint64_t len,
                                 rbd_completion_t c);
                                  }
}

pub fn aio_writesame(){
 unsafe{
 rbd_aio_writesame(rbd_image_t image, uint64_t off, size_t len,
                                   const char *buf, size_t data_len,
                                   rbd_completion_t c, int op_flags);
                                    }
}

pub fn aio_compare_and_write(){
 unsafe{
 ssize_t rbd_aio_compare_and_write(rbd_image_t image,
                                               uint64_t off, size_t len,
                                               const char *cmp_buf, const char *buf,
                                               rbd_completion_t c, uint64_t *mismatch_off,
                                               int op_flags);
                                                }

pub fn aio_create_completion(){
 unsafe{
 rbd_aio_create_completion(void *cb_arg,
                                           rbd_callback_t complete_cb,
                                           rbd_completion_t *c);
                                            }
}

pub fn aio_create_completion(){
 unsafe{
 rbd_aio_is_complete(rbd_completion_t c);
  }
}

pub fn aio_wait_for_complete(){
 unsafe{
 rbd_aio_wait_for_complete(rbd_completion_t c);
  }
}

pub fn aio_get_return_value(){
 unsafe{
 ssize_t rbd_aio_get_return_value(rbd_completion_t c);
  }
}

pub fn aio_get_arg(){
 unsafe{
 *rbd_aio_get_arg(rbd_completion_t c);
  }
}

pub fn aio_release(){
 unsafe{
 rbd_aio_release(rbd_completion_t c);
  }
}

///Start a flush if caching is enabled. Get a callback when
///the currently pending writes are on disk.
///@param image the image to flush writes to
///@param c what to call when flushing is complete
///@returns 0 on success, negative error code on failure
pub fn aio_flush(){
 unsafe{
 rbd_aio_flush(rbd_image_t image, rbd_completion_t c);
  }
}

pub fn poll_io_events(){
 unsafe{
 rbd_poll_io_events(rbd_image_t image, rbd_completion_t *comps, int numcomp);
  }
  }

pub fn metadata_get(){
 unsafe{
 rbd_metadata_get(rbd_image_t image, const char *key, char *value, size_t *val_len);
  }
  }

pub fn metadata_set(){
 unsafe{
 rbd_metadata_set(rbd_image_t image, const char *key, const char *value);
  }
  }

pub fn metadata_remove(){
 unsafe{
 rbd_metadata_remove(rbd_image_t image, const char *key);
  }
}
*/
    ///List all metadatas associated with this image.
    ///This iterates over all metadatas, key_len and val_len are filled in
    ///with the number of bytes put into the keys and values buffers.
    pub fn metadata_list(&self) -> RadosResult<HashMap<String, String>> {
        let mut metadata: HashMap<String, String> = HashMap::new();
        let mut keys: Vec<i8> = Vec::with_capacity(1024);
        let mut values: Vec<i8> = Vec::with_capacity(1024);

        let mut keys_len: usize = keys.capacity();
        let mut values_len: usize = values.capacity();
        let start = CString::new("")?;
        loop {
            unsafe {
                let retcode = rbd_metadata_list(
                    self.image,
                    start.as_ptr(),
                    0,
                    keys.as_mut_ptr(),
                    &mut keys_len,
                    values.as_mut_ptr(),
                    &mut values_len,
                );
                if retcode == -(nix::errno::Errno::ERANGE as i32) {
                    // provided byte array is smaller than listing size
                    //debug!("Resizing to {}", keys_len);
                    keys = Vec::with_capacity(keys_len);
                    values = Vec::with_capacity(values_len);
                    continue;
                }

                if retcode >= 0 {
                    // Set the buffer length to the size that ceph wrote to it
                    debug!(
                        "number of children (retcode): {}. Capacity: {}.  Setting len: {}",
                        retcode,
                        keys.capacity(),
                        keys_len,
                    );
                    keys.set_len(keys_len);
                    values.set_len(values_len);

                    // Insert into the map
                    let name: Vec<u8> = keys.iter().map(|c| c.clone() as u8).collect();
                    let val: Vec<u8> = values.iter().map(|c| c.clone() as u8).collect();
                    let name_str = String::from_utf8(name)?;
                    let val_str = String::from_utf8(val)?;
                    if !name_str.is_empty() && !val_str.is_empty() {
                        metadata.insert(name_str, val_str);
                    }
                    break;
                }
            }
        }
        Ok(metadata)
    }
    /*

pub fn mirror_aio_mirror_image_promote(){
 unsafe{
 rbd_aio_mirror_image_promote(rbd_image_t image, bool force,
                                              rbd_completion_t c);
                                               }
                                                                                    }
pub fn mirror_aio_mirror_image_demote(){
 unsafe{
 rbd_aio_mirror_image_demote(rbd_image_t image,
                                             rbd_completion_t c);
                                              }
                                                                                   }
pub fn mirror_aio_mirror_get_info(){
 unsafe{
 rbd_aio_mirror_image_get_info(rbd_image_t image,
                                               rbd_mirror_image_info_t *mirror_image_info,
                                               size_t info_size,
                                               rbd_completion_t c);
                                                }
                                                                                     }
pub fn mirror_aio_mirror_get_status(){
 unsafe{
 rbd_aio_mirror_image_get_status(rbd_image_t image,
                                                 rbd_mirror_image_status_t *mirror_image_status,
                                                 size_t status_size,
                                                 rbd_completion_t c);
                                                  }
                                     }

///Register an image metadata change watcher.
///@param image the image to watch
///@param handle where to store the internal id assigned to this watch
///@param watch_cb what to do when a notify is received on this image
///@param arg opaque value to pass to the callback
///@returns 0 on success, negative error code on failure
pub fn update_unwatch(){
 unsafe{
 rbd_update_watch(rbd_image_t image, uint64_t *handle,
				  rbd_update_callback_t watch_cb, void *arg);
                   }
}

///Unregister an image watcher.
///@param image the image to unwatch
///@param handle which watch to unregister
///@returns 0 on success, negative error code on failure
pub fn update_unwatch(){
 unsafe{
 rbd_update_unwatch(rbd_image_t image, uint64_t handle);
  }
                                     }

///List any watchers of an image.
///Watchers will be allocated and stored in the passed watchers array. If there
///are more watchers than max_watchers, -ERANGE will be returned and the number
///of watchers will be stored in max_watchers.
///The caller should call rbd_watchers_list_cleanup when finished with the list
///of watchers.
///@param image the image to list watchers for.
///@param watchers an array to store watchers in.
///@param max_watchers capacity of the watchers array.
///@returns 0 on success, negative error code on failure.
///@returns -ERANGE if there are too many watchers for the passed array.
///@returns the number of watchers in max_watchers.
 pub fn watchers_list(){
 unsafe{
 rbd_watchers_list(rbd_image_t image,
				   rbd_image_watcher_t *watchers,
				   size_t *max_watchers);
                    }
                                                         }

 pub fn watchers_list_cleanup(){
 unsafe{
 rbd_watchers_list_cleanup(rbd_image_watcher_t *watchers,
					    size_t num_watchers);
                         }
                                                              }

*/
}
