//! C FFI bindings exported for the Qt Basecamp app.
//! Called via document_indexer_ffi.h from whistleblower_impl.cpp.

use std::ffi::{c_char, c_int};
use std::slice;
use tokio::runtime::Runtime;

use crate::{Indexer, MetadataEnvelope};

/// Result type exposed over FFI.
#[repr(C)]
pub struct IndexResult {
    pub cid: [c_char; 64],
    pub metadata_hash: [u8; 32],
    pub success: c_int,
    pub error_msg: [c_char; 256],
}

impl IndexResult {
    fn ok(cid: &str, hash: [u8; 32]) -> Self {
        let mut r = IndexResult {
            cid: [0; 64],
            metadata_hash: hash,
            success: 1,
            error_msg: [0; 256],
        };
        let bytes = cid.as_bytes();
        let len = bytes.len().min(63);
        for (i, &b) in bytes[..len].iter().enumerate() {
            r.cid[i] = b as c_char;
        }
        r
    }

    fn err(msg: &str) -> Self {
        let mut r = IndexResult {
            cid: [0; 64],
            metadata_hash: [0; 32],
            success: 0,
            error_msg: [0; 256],
        };
        let bytes = msg.as_bytes();
        let len = bytes.len().min(255);
        for (i, &b) in bytes[..len].iter().enumerate() {
            r.error_msg[i] = b as c_char;
        }
        r
    }
}

fn cstr_to_string(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() }
}

#[no_mangle]
pub extern "C" fn document_indexer_upload_and_anchor(
    data: *const u8,
    data_len: usize,
    title: *const c_char,
    description: *const c_char,
    content_type: *const c_char,
    anchor_on_chain: c_int,
) -> IndexResult {
    let data_slice = unsafe { slice::from_raw_parts(data, data_len) }.to_vec();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let metadata = MetadataEnvelope {
        title: cstr_to_string(title),
        description: cstr_to_string(description),
        content_type: cstr_to_string(content_type),
        size_bytes: data_len as u64,
        timestamp,
        tags: vec![],
    };

    let rt = match Runtime::new() {
        Ok(r) => r,
        Err(e) => return IndexResult::err(&format!("tokio runtime: {e}")),
    };

    let indexer = Indexer::default();
    match rt.block_on(indexer.upload_and_broadcast(&data_slice, metadata)) {
        Ok(result) => {
            if anchor_on_chain != 0 {
                let entries = vec![(result.cid.clone(), result.metadata_hash)];
                let _ = rt.block_on(indexer.anchor_batch(entries, 0));
            }
            IndexResult::ok(&result.cid, result.metadata_hash)
        }
        Err(e) => IndexResult::err(&format!("{e:#}")),
    }
}

#[no_mangle]
pub extern "C" fn document_indexer_is_anchored(cid: *const c_char) -> c_int {
    let cid_str = cstr_to_string(cid);
    let rt = match Runtime::new() {
        Ok(r) => r,
        Err(_) => return 0,
    };
    let indexer = Indexer::default();
    if rt.block_on(indexer.is_anchored(&cid_str)).unwrap_or(false) {
        1
    } else {
        0
    }
}
