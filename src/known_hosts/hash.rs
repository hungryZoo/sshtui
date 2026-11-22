//! SHA-1, HMAC-SHA1, and base64 decoding for matching hashed host names only.

const BLOCK_SIZE: usize = 64;

/// Returns the SHA-1 digest of `data`.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    const LENGTH_OFFSET: usize = BLOCK_SIZE - size_of::<u64>();
    let mut state = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    let (blocks, remainder) = data.as_chunks::<BLOCK_SIZE>();
    for block in blocks {
        compress(&mut state, block);
    }

    let mut final_block = [0; BLOCK_SIZE];
    for (byte, value) in final_block
        .iter_mut()
        .zip(remainder.iter().copied().chain([0x80]))
    {
        *byte = value;
    }
    let bit_length = (data.len() as u64).wrapping_mul(8).to_be_bytes();
    if remainder.len() >= LENGTH_OFFSET {
        compress(&mut state, &final_block);
        final_block.fill(0);
    }
    final_block[LENGTH_OFFSET..].copy_from_slice(&bit_length);
    compress(&mut state, &final_block);

    let mut digest = [0; 20];
    for (bytes, word) in digest.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn compress(state: &mut [u32; 5], block: &[u8; BLOCK_SIZE]) {
    let mut words = [0u32; 80];
    let (block_words, _) = block.as_chunks::<4>();
    for (word, bytes) in words.iter_mut().zip(block_words) {
        *word = u32::from_be_bytes(*bytes);
    }
    for i in 16..words.len() {
        words[i] = (words[i - 3] ^ words[i - 8] ^ words[i - 14] ^ words[i - 16]).rotate_left(1);
    }

    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (i, word) in words.into_iter().enumerate() {
        let (f, constant) = match i {
            0..=19 => ((b & c) | (!b & d), 0x5a827999),
            20..=39 => (b ^ c ^ d, 0x6ed9eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
            _ => (b ^ c ^ d, 0xca62c1d6),
        };
        let next = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(constant)
            .wrapping_add(word);
        [a, b, c, d, e] = [next, a, b.rotate_left(30), c, d];
    }

    for (word, result) in state.iter_mut().zip([a, b, c, d, e]) {
        *word = word.wrapping_add(result);
    }
}

/// Returns HMAC-SHA1 of `data` under `key` (RFC 2104).
pub fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let hashed_key;
    let key = if key.len() > BLOCK_SIZE {
        hashed_key = sha1(key);
        hashed_key.as_slice()
    } else {
        key
    };
    let mut inner = Vec::from([0x36; BLOCK_SIZE]);
    let mut outer = [0x5c; BLOCK_SIZE + 20];
    for ((inner_byte, outer_byte), &key_byte) in inner.iter_mut().zip(&mut outer).zip(key) {
        *inner_byte ^= key_byte;
        *outer_byte ^= key_byte;
    }
    inner.extend_from_slice(data);
    outer[BLOCK_SIZE..].copy_from_slice(&sha1(&inner));
    sha1(&outer)
}

/// Decodes standard base64 with `=` padding. Returns `None` for any invalid input.
pub fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let (groups, remainder) = input.as_chunks::<4>();
    if !remainder.is_empty() {
        return None;
    }

    let mut decoded = Vec::with_capacity(groups.len() * 3);
    let mut groups = groups.iter().peekable();
    while let Some(&[first, second, third, fourth]) = groups.next() {
        let first = base64_value(first)?;
        let second = base64_value(second)?;
        decoded.push((first << 2) | (second >> 4));
        if third == b'=' && fourth == b'=' && second & 0x0f == 0 && groups.peek().is_none() {
            break;
        }

        let third = base64_value(third)?;
        decoded.push((second << 4) | (third >> 2));
        if fourth == b'=' && third & 0x03 == 0 && groups.peek().is_none() {
            break;
        }

        let fourth = base64_value(fourth)?;
        decoded.push((third << 6) | fourth);
    }
    Some(decoded)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(digest: [u8; 20]) -> String {
        use std::fmt::Write;

        let mut hex = String::new();
        for byte in digest {
            write!(hex, "{byte:02x}").unwrap();
        }
        hex
    }

    #[test]
    fn sha1_matches_reference_digests() {
        for (data, expected) in [
            (b"".as_slice(), "da39a3ee5e6b4b0d3255bfef95601890afd80709"),
            (b"abc", "a9993e364706816aba3e25717850c26c9cd0d89d"),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "84983e441c3bd26ebaae4aa1f95129e5e54670f1",
            ),
            (
                &vec![b'a'; 1_000_000],
                "34aa973cd4c4daa4f61eeb2bdbad27316534016f",
            ),
        ] {
            assert_eq!(hex(sha1(data)), expected);
        }
    }

    #[test]
    fn hmac_sha1_matches_reference_digests() {
        for (key, data, expected) in [
            (
                [0x0b; 20].as_slice(),
                b"Hi There".as_slice(),
                "b617318655057264e28bc0b6fb378c8ef146be00",
            ),
            (
                b"Jefe",
                b"what do ya want for nothing?",
                "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79",
            ),
            (
                &[0xaa; 20],
                &[0xdd; 50],
                "125d7342b9ac11cd91a39af48aa17b4f63f175d3",
            ),
            (
                &[0xaa; 80],
                b"Test Using Larger Than Block-Size Key - Hash Key First",
                "aa4ae5e15272d00e95705637ce8a3b55ed402112",
            ),
        ] {
            assert_eq!(hex(hmac_sha1(key, data)), expected);
        }
    }

    #[test]
    fn base64_decodes_valid_inputs() {
        for (input, expected) in [
            (b"Zm9vYmFy".as_slice(), b"foobar".as_slice()),
            (b"Zm9vYg==", b"foob"),
            (b"Zm9vYmE=", b"fooba"),
            (b"+///", b"\xfb\xff\xff"),
            (b"", b""),
        ] {
            assert_eq!(base64_decode(input).as_deref(), Some(expected));
        }
    }

    #[test]
    fn base64_rejects_invalid_inputs() {
        for input in [
            b"Zm9v=".as_slice(),
            b"Zm9vY",
            b"Zm9v Yg==",
            b"Zm9v Yg=",
            b"Zg=a",
            b"====",
            b"Z===",
            b"Zg==Zm9v",
            b"Zm8=Zm9v",
            b"Z#==",
            b"Zh==",
            b"Zm9=",
        ] {
            assert_eq!(base64_decode(input), None);
        }
    }
}
