//! SHA-256, hand-rolled, for one job: identifying a benchmark's prompt.
//!
//! A benchmark report exists to be FALSIFIED rather than believed (the same
//! rule that keeps llama-bench's raw row and the driver versions in the file),
//! and with the prompt moved out into a file that anyone can edit between two
//! runs, "same prompt" stops being something a report can assert by embedding
//! a few hundred characters. A digest can assert it in one line, whatever the
//! prompt's size.
//!
//! Hand-rolled rather than pulled in: `sha2` costs six crates
//! (digest / block-buffer / crypto-common / generic-array / typenum /
//! cpufeatures) for one call site with no performance requirement, against
//! ~70 lines pinned by the standard test vectors below.
//!
//! One thing to know at the call site: `bench` hashes the NORMALIZED prompt
//! text (BOM stripped, CRLF folded to LF), not the file's bytes, because the
//! normalized text is what reaches the model and therefore what decides whether
//! two runs are comparable. So this digest deliberately does NOT match
//! `Get-FileHash` on a CRLF-terminated file, and the report says so.

/// The 64 round constants: the first 32 bits of the fractional parts of the
/// cube roots of the first 64 primes (FIPS 180-4, section 4.2.2).
#[rustfmt::skip]
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The digest of `data` as 64 lowercase hex characters.
pub fn hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Padding: the 0x80 terminator, zeroes up to 56 bytes mod 64, then the
    // message length in BITS as a big-endian u64.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    // The padding above makes the length a multiple of 64, so the remainder
    // `as_chunks` also returns is always empty; the fixed-size chunk is what
    // clippy asks for over `chunks_exact(64)` (chunks_exact_to_as_chunks).
    for chunk in msg.as_chunks::<64>().0 {
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let b = &chunk[i * 4..i * 4 + 4];
            *word = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let x = w[i - 15];
            let y = w[i - 2];
            let s0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
            let s1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (kv, wv) in K.iter().zip(w.iter()) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*kv)
                .wrapping_add(*wv);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        for (slot, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(add);
        }
    }

    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{word:08x}"));
    }
    out
}

/// The first 12 hex characters, for a readout that has to fit next to a path.
/// Long enough to be worth glancing at, short enough not to push the path out
/// of the line; the report always carries the full 64.
pub fn short(data: &[u8]) -> String {
    let full = hex(data);
    full[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published FIPS 180-4 / NIST vectors. Every digest here is a constant
    /// from the specification or from its widely reproduced test set, never one
    /// this implementation produced: a value read back out of the code under
    /// test pins the bug instead of the algorithm.
    #[test]
    fn known_vectors() {
        let cases: &[(&str, &str)] = &[
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            // 56 bytes: the first length whose padding needs a SECOND block,
            // i.e. the case a `< 56` pad loop gets wrong.
            (
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
            // 112 bytes: two full blocks of message plus a third of padding.
            (
                "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmn\
                 hijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
                "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(&hex(input.as_bytes()), want, "sha256({input:?})");
        }
    }

    /// A million 'a's: the one vector that exercises many blocks in a row, and
    /// the only place a carry bug between blocks would show up.
    #[test]
    fn one_million_a() {
        let msg = "a".repeat(1_000_000);
        assert_eq!(
            hex(msg.as_bytes()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn short_is_the_prefix_of_hex() {
        let data = b"benchmark prompt";
        assert_eq!(short(data), hex(data)[..12]);
        assert_eq!(short(data).len(), 12);
    }
}
