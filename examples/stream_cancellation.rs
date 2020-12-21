//! This example highlights Isahc's async streaming capabilities by implementing
//! a program that aborts downloading a response if it contains the byte `0x2B`
//! (ASCII "+").

use futures_lite::{future::block_on, io::AsyncReadExt};
use isahc::prelude::*;

fn main() -> Result<(), isahc::Error> {
    block_on(async {
        // Open a response stream.
        let mut response = isahc::get_async("https://blog.rust-lang.org").await?;

        let mut buf = [0; 8192];
        let mut offset = 0;
        let reader = response.body_mut();

        // Set up a loop where we continuously read from the stream.
        loop {
            match reader.read(&mut buf).await? {
                // Zero bytes read, we hit EOF with no plus signs.
                0 => {
                    println!("Download complete! No '+' byte of all {} bytes.", offset);
                    return Ok(());
                }
                // At least one byte was read.
                len => {
                    // Check to see if there's any plus signs this time around.
                    for &byte in &buf[..len] {
                        if byte == b'+' {
                            println!("Abort, saw a '+' at offset {}!", offset);

                            // We can just return here and let the drop handler
                            // abort the response, but it is better to call
                            // `abort()` explicitly.
                            response.abort();

                            return Ok(());
                        }
                        // Keep track of how many bytes we've checked so far.
                        offset += 1;
                    }
                }
            }
        }

        // If we did not read the entire stream before returning, when the
        // response is dropped the download will be aborted.
    })
}
