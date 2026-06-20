#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// C FFI bindings for the document-indexer Rust library.
// These are called from whistleblower_impl.cpp.

typedef struct IndexResult {
    char cid[64];           // base58-encoded CID, null-terminated
    uint8_t metadata_hash[32];
    int success;            // 1 = ok, 0 = error
    char error_msg[256];    // populated on failure
} IndexResult;

// Upload data to Logos Storage, broadcast to Delivery, and anchor on-chain.
// title/description/tags are null-terminated UTF-8 strings.
// anchor_on_chain: 1 = also submit batch anchor, 0 = upload only
IndexResult document_indexer_upload_and_anchor(
    const uint8_t* data,
    size_t data_len,
    const char* title,
    const char* description,
    const char* content_type,
    int anchor_on_chain
);

// Returns 1 if the given CID (null-terminated base58) is registered on-chain.
int document_indexer_is_anchored(const char* cid);

#ifdef __cplusplus
}
#endif
