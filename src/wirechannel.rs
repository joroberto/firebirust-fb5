// MIT License
//
// Copyright (c) 2021 Hajime Nakagami<nakagami@gmail.com>
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

use super::compression::WireCompressor;
use super::crypt_translater::{Arc4, ChaCha, CryptTranslator};
use super::error::Error;
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use hex;
use std::collections::VecDeque;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;

pub struct WireChannel {
    stream: TcpStream,  // Keep reference for timeout control
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
    read_buf: VecDeque<u8>,  // VecDeque for O(1) pop_front
    read_trans: Option<Box<dyn CryptTranslator>>,
    write_trans: Option<Box<dyn CryptTranslator>>,
    compressor: Option<WireCompressor>,
    compressed: bool,
}

impl WireChannel {
    pub fn new(host: &str, port: u16) -> Result<WireChannel, Error> {
        let stream = TcpStream::connect(format!("{}:{}", host, port))?;
        // CRITICAL: Disable Nagle's algorithm for low-latency operations
        stream.set_nodelay(true)?;
        // Buffer size matching fbclient's MAX_DATA_HW (32KB)
        const BUFFER_SIZE: usize = 32768;
        let reader = BufReader::with_capacity(BUFFER_SIZE, stream.try_clone()?);
        let writer = BufWriter::with_capacity(BUFFER_SIZE, stream.try_clone()?);
        Ok(WireChannel {
            stream,
            reader,
            writer,
            read_buf: VecDeque::with_capacity(BUFFER_SIZE),
            read_trans: None,
            write_trans: None,
            compressor: None,
            compressed: false,
        })
    }

    /// Set read timeout for the underlying socket
    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> Result<(), Error> {
        self.stream.set_read_timeout(timeout)?;
        Ok(())
    }

    /// Enable wire compression
    pub fn enable_compression(&mut self) {
        self.compressor = Some(WireCompressor::new());
        self.compressed = true;
    }

    /// Check if compression is enabled
    pub fn is_compressed(&self) -> bool {
        self.compressed
    }

    pub fn set_crypt_key(&mut self, plugin: &[u8], key: &[u8], nonce: &[u8]) {
        if plugin == b"ChaCha64" || plugin == b"ChaCha" {
            let mut hasher = Sha256::new();
            hasher.input(&key);
            let key = &hex::decode(hasher.result_str()).unwrap();
            self.read_trans = Some(Box::new(ChaCha::new(key, nonce)));
            self.write_trans = Some(Box::new(ChaCha::new(key, nonce)));
        } else if plugin == b"Arc4" {
            self.read_trans = Some(Box::new(Arc4::new(key)));
            self.write_trans = Some(Box::new(Arc4::new(key)));
        }
    }

    pub fn read(&mut self, n: usize) -> Result<Vec<u8>, Error> {
        // Fill buffer if needed
        while self.read_buf.len() < n {
            let mut input_buf = [0u8; 8192];
            let ln = self.reader.read(&mut input_buf)?;
            if ln == 0 {
                return Err(Error::IoError(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Connection closed",
                )));
            }

            // Apply decryption first if enabled
            let decrypted = if let Some(ref mut trans) = self.read_trans {
                trans.translate(&input_buf[..ln]).to_vec()
            } else {
                input_buf[..ln].to_vec()
            };

            // Then apply decompression if enabled
            let data = if self.compressed {
                if let Some(ref mut comp) = self.compressor {
                    comp.decompress(&decrypted)?
                } else {
                    decrypted
                }
            } else {
                decrypted
            };

            self.read_buf.extend(&data);
        }

        // Efficient extraction using drain - O(n) instead of O(n²)
        let v: Vec<u8> = self.read_buf.drain(..n).collect();
        Ok(v)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<(), Error> {
        // Apply compression first if enabled
        let compressed = if self.compressed {
            if let Some(ref mut comp) = self.compressor {
                comp.compress(buf)?
            } else {
                buf.to_vec()
            }
        } else {
            buf.to_vec()
        };

        // Then apply encryption if enabled
        if let Some(ref mut trans) = self.write_trans {
            self.writer.write_all(&*trans.translate(&compressed))?;
        } else {
            self.writer.write_all(&compressed)?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.writer.flush()?;
        Ok(())
    }
}
