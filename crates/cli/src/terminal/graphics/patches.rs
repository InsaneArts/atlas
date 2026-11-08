//! Optional Kitty root-frame edits. These overwrite frame 1 instead of adding
//! animation frames, so memory stays bounded for a long-running desktop view.
use super::{delete_image, payload, upload as upload_image};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::{self, Write};

const QUERY: u32 = 47474;

pub(super) fn upload(
    out: &mut impl Write,
    image: u32,
    rgb: &[u8],
    size: (u32, u32),
    origin: (u32, u32),
    quiet: u8,
) -> io::Result<()> {
    let payload = payload::compress(rgb, size.0)?;
    let mut chunks = payload.chunks(3072).peekable();
    let mut encoded = [0; 4096];
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        if first {
            write!(
                out,
                "\x1b_Ga=f,i={image},r=1,X=1,x={},y={},f=24,s={},v={},o=z,t=d,q={quiet},m={more};",
                origin.0, origin.1, size.0, size.1
            )?;
            first = false;
        } else {
            // Unlike image uploads, every animation-data continuation needs
            // a=f. Without it a large rectangle can corrupt the transmission.
            write!(out, "\x1b_Ga=f,q={quiet},m={more};")?;
        }
        let length = STANDARD
            .encode_slice(chunk, &mut encoded)
            .map_err(io::Error::other)?;
        out.write_all(&encoded[..length])?;
        out.write_all(b"\x1b\\")?;
    }
    Ok(())
}

/// Never create a placement: the probe cannot display or disturb a picture.
/// q=2 suppresses the base upload's response; only the root edit may say OK.
pub(in super::super) fn patch_query(out: &mut impl Write) -> io::Result<()> {
    upload_image(out, &[0, 0, 0], QUERY, (1, 1))?;
    upload(out, QUERY, &[1, 2, 3], (1, 1), (0, 0), 0)
}

pub(in super::super) fn patch_query_clear(out: &mut impl Write) -> io::Result<()> {
    delete_image(out, QUERY)
}

pub(in super::super) fn patch_supported(reply: &[u8]) -> bool {
    let Ok(reply) = std::str::from_utf8(reply) else {
        return false;
    };
    let expected = format!("i={QUERY}");
    reply.split("\x1b_G").skip(1).any(|command| {
        let Some((command, _)) = command.split_once("\x1b\\") else {
            return false;
        };
        command.split_once(';').is_some_and(|(keys, status)| {
            status == "OK" && keys.split(',').any(|key| key == expected)
        })
    })
}
