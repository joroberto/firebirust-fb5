// MIT License
//
// Copyright (c) 2021 Hajime Nakagami<nakagami@gmail.com>
// Copyright (c) 2026 Roberto (wire compression implementation)
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Wire Compression for Firebird protocol
//!
//! This module provides zlib-based compression for the Firebird wire protocol.
//! Compression is negotiated during the connection handshake and, if enabled,
//! all subsequent packets are compressed using zlib deflate/inflate.

use flate2::write::{ZlibDecoder, ZlibEncoder};
use flate2::Compression;
use std::io::Write;

use super::error::Error;

/// Wire compressor using zlib (deflate/inflate)
///
/// Firebird wire compression uses raw zlib streams with a shared dictionary
/// that persists across packets. Each packet is compressed incrementally
/// and ends with a Z_SYNC_FLUSH marker.
pub struct WireCompressor {
    encoder: ZlibEncoder<Vec<u8>>,
    decoder: ZlibDecoder<Vec<u8>>,
}

impl WireCompressor {
    /// Create a new wire compressor with default compression level
    pub fn new() -> Self {
        Self {
            encoder: ZlibEncoder::new(Vec::new(), Compression::default()),
            decoder: ZlibDecoder::new(Vec::new()),
        }
    }

    /// Compress data using zlib deflate
    ///
    /// The compression maintains state across calls (streaming compression),
    /// which matches Firebird's wire compression behavior.
    pub fn compress(&mut self, data: &[u8]) -> Result<Vec<u8>, Error> {
        // Reset output buffer
        self.encoder.get_mut().clear();

        // Write data to compressor
        self.encoder.write_all(data)?;

        // Flush with sync flush to get compressed data
        self.encoder.flush()?;

        // Get compressed data
        let compressed = self.encoder.get_ref().clone();

        Ok(compressed)
    }

    /// Decompress data using zlib inflate
    ///
    /// The decompression maintains state across calls (streaming decompression),
    /// which matches Firebird's wire compression behavior.
    pub fn decompress(&mut self, data: &[u8]) -> Result<Vec<u8>, Error> {
        // Reset output buffer
        self.decoder.get_mut().clear();

        // Write compressed data to decoder
        self.decoder.write_all(data)?;

        // Flush to get decompressed data
        self.decoder.flush()?;

        // Get decompressed data
        let decompressed = self.decoder.get_ref().clone();

        Ok(decompressed)
    }
}

impl Default for WireCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress() {
        let mut compressor = WireCompressor::new();

        let original = b"Hello, World! This is a test of wire compression for Firebird.";

        let compressed = compressor.compress(original).unwrap();

        // Compressed data should be different from original
        // (though for small data it might be larger due to zlib header)

        // Create a new compressor for decompression test
        let mut decompressor = WireCompressor::new();
        let decompressed = decompressor.decompress(&compressed).unwrap();

        assert_eq!(original.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_compress_large_data() {
        let mut compressor = WireCompressor::new();

        // Create large repetitive data (compresses well)
        let original: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let compressed = compressor.compress(&original).unwrap();

        // Repetitive data should compress significantly
        assert!(compressed.len() < original.len());

        // Verify decompression
        let mut decompressor = WireCompressor::new();
        let decompressed = decompressor.decompress(&compressed).unwrap();

        assert_eq!(original, decompressed);
    }
}
