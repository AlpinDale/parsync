const MOD: u32 = 1 << 16;

#[derive(Debug, Clone)]
pub struct RollingChecksum {
    a: u32,
    b: u32,
    len: usize,
}

impl RollingChecksum {
    pub fn new(buf: &[u8]) -> Self {
        let mut a = 0_u32;
        let mut b = 0_u32;
        for (i, &byte) in buf.iter().enumerate() {
            a = (a + byte as u32) % MOD;
            b = (b + ((buf.len() - i) as u32 * byte as u32)) % MOD;
        }
        Self {
            a,
            b,
            len: buf.len(),
        }
    }

    pub fn roll(&mut self, out: u8, inn: u8) {
        let out_u = out as u32;
        let in_u = inn as u32;
        self.a = (MOD + self.a + in_u - out_u) % MOD;
        self.b = (MOD + self.b + self.a - ((self.len as u32 * out_u) % MOD)) % MOD;
    }

    pub fn sum(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

pub fn weak_hash(buf: &[u8]) -> u32 {
    RollingChecksum::new(buf).sum()
}

pub fn strong_hash128(buf: &[u8]) -> u128 {
    let digest = md5::compute(buf);
    u128::from_be_bytes(digest.0)
}

#[cfg(test)]
mod tests {
    use super::{weak_hash, RollingChecksum};

    #[test]
    fn rolling_matches_full_recompute() {
        let data = b"abcdefghijklmnopqrstuvwxyz";
        let win = 8;
        let mut r = RollingChecksum::new(&data[..win]);
        for i in 0..=(data.len() - win) {
            let full = weak_hash(&data[i..i + win]);
            assert_eq!(r.sum(), full);
            if i + win < data.len() {
                r.roll(data[i], data[i + win]);
            }
        }
    }
}
