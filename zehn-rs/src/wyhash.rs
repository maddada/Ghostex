//! Byte-exact port of Zig 0.16's `std.hash.Wyhash`.
//!
//! CDXC:AgentHistorySearch 2026-08-20:
//! The favorites file stores one 64-bit key per starred prompt, and the Codex
//! derived-cache filenames are hashes of the source path. Both were produced by
//! Zig's Wyhash. Porting the hash byte-for-byte is what lets the Rust build keep
//! reading the favorites and cache files the Zig build wrote, instead of
//! silently un-starring every favorite and re-parsing every Codex transcript.

const SECRET: [u64; 4] = [
    0xa076_1d64_78bd_642f,
    0xe703_7ed1_a0b4_28db,
    0x8ebc_6af0_9c88_c6e3,
    0x5899_65cc_7537_4cc3,
];

#[inline]
fn mum(a: &mut u64, b: &mut u64) {
    let x = (*a as u128).wrapping_mul(*b as u128);
    *a = x as u64;
    *b = (x >> 64) as u64;
}

#[inline]
fn mix(a_: u64, b_: u64) -> u64 {
    let mut a = a_;
    let mut b = b_;
    mum(&mut a, &mut b);
    a ^ b
}

#[inline]
fn read(bytes: usize, data: &[u8]) -> u64 {
    debug_assert!(bytes <= 8);
    let mut buf = [0u8; 8];
    buf[..bytes].copy_from_slice(&data[..bytes]);
    u64::from_le_bytes(buf)
}

#[derive(Clone)]
pub struct Wyhash {
    a: u64,
    b: u64,
    state: [u64; 3],
    total_len: usize,
    buf: [u8; 48],
    buf_len: usize,
}

impl Wyhash {
    pub fn new(seed: u64) -> Self {
        let s0 = seed ^ mix(seed ^ SECRET[0], SECRET[1]);
        Self { a: 0, b: 0, state: [s0, s0, s0], total_len: 0, buf: [0u8; 48], buf_len: 0 }
    }

    pub fn update(&mut self, input: &[u8]) {
        self.total_len += input.len();

        if input.len() <= 48 - self.buf_len {
            self.buf[self.buf_len..self.buf_len + input.len()].copy_from_slice(input);
            self.buf_len += input.len();
            return;
        }

        let mut i: usize = 0;

        if self.buf_len > 0 {
            i = 48 - self.buf_len;
            let buf_len = self.buf_len;
            self.buf[buf_len..buf_len + i].copy_from_slice(&input[..i]);
            let block = self.buf;
            self.round(&block);
            self.buf_len = 0;
        }

        while i + 48 < input.len() {
            let mut block = [0u8; 48];
            block.copy_from_slice(&input[i..i + 48]);
            self.round(&block);
            i += 48;
        }

        let remaining = &input[i..];
        // Wyhash's streaming form keeps the 16 bytes that precede a short tail,
        // parked at the end of the scratch buffer, so `final` can reconstruct the
        // last full 16-byte window.
        if remaining.len() < 16 && i >= 48 {
            let rem = 16 - remaining.len();
            self.buf[48 - rem..].copy_from_slice(&input[i - rem..i]);
        }
        self.buf[..remaining.len()].copy_from_slice(remaining);
        self.buf_len = remaining.len();
    }

    pub fn finish(&self) -> u64 {
        let mut me = self.clone();
        let buf = self.buf;
        let buf_len = self.buf_len;

        if self.total_len <= 16 {
            me.small_key(&buf[..buf_len]);
        } else {
            let mut scratch = [0u8; 16];
            let (input, offset): (&[u8], usize) = if buf_len < 16 {
                let rem = 16 - buf_len;
                scratch[..rem].copy_from_slice(&buf[48 - rem..]);
                scratch[rem..rem + buf_len].copy_from_slice(&buf[..buf_len]);
                (&scratch[..], rem)
            } else {
                (&buf[..buf_len], 0)
            };
            me.final0();
            me.final1(input, offset);
        }

        me.final2()
    }

    fn small_key(&mut self, input: &[u8]) {
        debug_assert!(input.len() <= 16);
        if input.len() >= 4 {
            let end = input.len() - 4;
            let quarter = (input.len() >> 3) << 2;
            self.a = (read(4, input) << 32) | read(4, &input[quarter..]);
            self.b = (read(4, &input[end..]) << 32) | read(4, &input[end - quarter..]);
        } else if !input.is_empty() {
            self.a = ((input[0] as u64) << 16)
                | ((input[input.len() >> 1] as u64) << 8)
                | (input[input.len() - 1] as u64);
            self.b = 0;
        } else {
            self.a = 0;
            self.b = 0;
        }
    }

    fn round(&mut self, input: &[u8; 48]) {
        for i in 0..3 {
            let a = read(8, &input[8 * (2 * i)..]);
            let b = read(8, &input[8 * (2 * i + 1)..]);
            self.state[i] = mix(a ^ SECRET[i + 1], b ^ self.state[i]);
        }
    }

    fn final0(&mut self) {
        self.state[0] ^= self.state[1] ^ self.state[2];
    }

    fn final1(&mut self, input_lb: &[u8], start_pos: usize) {
        debug_assert!(input_lb.len() >= 16);
        let input = &input_lb[start_pos..];
        let mut i: usize = 0;
        while i + 16 < input.len() {
            self.state[0] = mix(read(8, &input[i..]) ^ SECRET[1], read(8, &input[i + 8..]) ^ self.state[0]);
            i += 16;
        }
        self.a = read(8, &input_lb[input_lb.len() - 16..]);
        self.b = read(8, &input_lb[input_lb.len() - 8..]);
    }

    fn final2(&mut self) -> u64 {
        self.a ^= SECRET[1];
        self.b ^= self.state[0];
        let (mut a, mut b) = (self.a, self.b);
        mum(&mut a, &mut b);
        self.a = a;
        self.b = b;
        mix(self.a ^ SECRET[0] ^ (self.total_len as u64), self.b ^ SECRET[1])
    }

    pub fn hash(seed: u64, input: &[u8]) -> u64 {
        let mut me = Wyhash::new(seed);
        if input.len() <= 16 {
            me.small_key(input);
        } else {
            let mut i: usize = 0;
            if input.len() >= 48 {
                while i + 48 < input.len() {
                    let mut block = [0u8; 48];
                    block.copy_from_slice(&input[i..i + 48]);
                    me.round(&block);
                    i += 48;
                }
                me.final0();
            }
            me.final1(input, i);
        }
        me.total_len = input.len();
        me.final2()
    }
}

#[cfg(test)]
mod tests {
    use super::Wyhash;

    /// Vectors captured from Zig 0.16 `std.hash.Wyhash` (the implementation that
    /// wrote every existing favorites file and Codex cache filename).
    #[test]
    fn matches_zig_wyhash_vectors() {
        let cases: [(&[u8], u64); 9] = [
            (b"", 0x0409638ee2bde459),
            (b"a", 0x28d2053309d28531),
            (b"abc", 0x02a4f1d7cb516c72),
            (b"fix the bug", 0x2547b5d7d861f947),
            (b"claude\x00fix the bug", 0x08775375308e09c0),
            (b"0123456789abcdef", 0xc304e72c387cd229),
            (b"0123456789abcdefg", 0xb496f8f306600195),
            (
                b"The quick brown fox jumps over the lazy dog and keeps running for a very long time indeed",
                0x83d359a685c7d53a,
            ),
            (&[b'x'; 200], 0x0a84415acf63cb87),
        ];
        for (input, want) in cases {
            let mut h = Wyhash::new(0);
            h.update(input);
            assert_eq!(h.finish(), want, "streamed hash of {} bytes", input.len());
            assert_eq!(Wyhash::hash(0, input), want, "one-shot hash of {} bytes", input.len());
        }
    }

    #[test]
    fn matches_zig_when_streamed_in_pieces() {
        let mut h = Wyhash::new(0);
        h.update(b"claude");
        h.update(&[0]);
        h.update(b"fix the bug");
        assert_eq!(h.finish(), 0x08775375308e09c0);

        let mut p = Wyhash::new(0);
        p.update(b"/Users/madda/.codex/sessions/2026/08/20/rollout-2026-08-20T10-00-00-abcdef.jsonl");
        assert_eq!(p.finish(), 0xb39b8f78d9344edf);
    }
}
