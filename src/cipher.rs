// Stars! L'Ecuyer (1988) combined LCG cipher.
//
// All data file payloads (type 6, type 16, etc.) are XOR-encrypted with this
// cipher.  Type-8 (file header) records are plaintext and seed the state.
//
// Cipher formula (confirmed by differential analysis of six default race files):
//   Per 4-byte chunk: key = (new_s1 − new_s2) as a 32-bit unsigned wrap.
//   The four LE bytes of that u32 are XORed with the four plaintext bytes.
//
// LCG constants:
//   s1: a=40014, m=2^31−85,  q=53668, r=12211  (Schrage method)
//   s2: a=40692, m=2^31−249, q=52774, r=3791

/// 64-entry prime table used to derive initial LCG seeds from the file header.
pub const SEED_TABLE: [i64; 64] = [
    3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59,
    61, 67, 71, 73, 79, 83, 89, 97, 101, 103, 107, 109, 113, 127, 131, 137,
    139, 149, 151, 157, 163, 167, 173, 179, 181, 191, 193, 197, 199, 211, 223, 227,
    229, 233, 239, 241, 251, 257, 263, 279, 271, 277, 281, 283, 293, 307, 311, 313,
];

const M1: i64 = 2_147_483_563; // 2^31 − 85
const M2: i64 = 2_147_483_399; // 2^31 − 249
const A1: i64 = 40_014; const Q1: i64 = 53_668; const R1: i64 = 12_211;
const A2: i64 = 40_692; const Q2: i64 = 52_774; const R2: i64 = 3_791;

/// L'Ecuyer combined LCG state.
pub struct LcgState {
    pub s1: i64,
    pub s2: i64,
}

impl LcgState {
    pub fn new(s1: i64, s2: i64) -> Self { Self { s1, s2 } }

    fn step_s1(&mut self) {
        let k = self.s1 / Q1;
        let t = A1 * (self.s1 - k * Q1) - k * R1;
        self.s1 = if t < 0 { t + M1 } else { t };
    }

    fn step_s2(&mut self) {
        let k = self.s2 / Q2;
        let t = A2 * (self.s2 - k * Q2) - k * R2;
        self.s2 = if t < 0 { t + M2 } else { t };
    }

    /// Return the next 4-byte XOR key dword and advance the LCG state.
    /// key = (new_s1 − new_s2) as a 32-bit unsigned wrap.
    pub fn next_key_dword(&mut self) -> u32 {
        self.step_s1();
        self.step_s2();
        (self.s1 - self.s2) as u32
    }

    /// Advance the state `n` steps without returning key values.
    pub fn advance(&mut self, n: usize) {
        for _ in 0..n { self.next_key_dword(); }
    }
}

/// Derive initial (s1, s2) seeds from the 16-bit seed word at bytes 12-13 of
/// the type-8 file header payload.
pub fn derive_seeds(seed_word: u16) -> (i64, i64) {
    let param3 = (seed_word >> 5) as u32;
    let mut idx1 = (param3 & 0x1F) as usize;
    let mut idx2 = ((param3 >> 5) & 0x1F) as usize;
    if (param3 & 0x400) == 0 { idx2 += 32; } else { idx1 += 32; }
    (SEED_TABLE[idx1], SEED_TABLE[idx2])
}

/// Compute the pre-advance count from the 16-byte type-8 file header payload.
/// The LCG is advanced this many steps after seeding, before any record is
/// decrypted.
pub fn derive_pre_advance(hdr: &[u8]) -> usize {
    let p1 = i16::from_le_bytes([hdr[4], hdr[5]]) as i64;
    let p4 = i16::from_le_bytes([hdr[10], hdr[11]]) as i64;
    let sw = u16::from_le_bytes([hdr[12], hdr[13]]) as u64;
    let p5 = (sw & 0x1F) as i64;
    let p6_word = u16::from_le_bytes([hdr[14], hdr[15]]) as u64;
    let p6 = ((p6_word >> 12) & 1) as i64;
    (((p1 & 3) + 1) * ((p4 & 3) + 1) * ((p5 & 3) + 1) + p6) as usize
}

/// XOR-decrypt `data` in-place using `lcg` as the key stream.
pub fn decrypt(data: &mut [u8], lcg: &mut LcgState) {
    let chunks = data.len() / 4;
    for i in 0..chunks {
        let key = lcg.next_key_dword();
        let b = i * 4;
        data[b]     ^= (key        & 0xFF) as u8;
        data[b + 1] ^= ((key >>  8) & 0xFF) as u8;
        data[b + 2] ^= ((key >> 16) & 0xFF) as u8;
        data[b + 3] ^= ((key >> 24) & 0xFF) as u8;
    }
    let rem = data.len() % 4;
    if rem > 0 {
        let key = lcg.next_key_dword();
        let b = chunks * 4;
        for i in 0..rem {
            data[b + i] ^= ((key >> (i * 8)) & 0xFF) as u8;
        }
    }
}
