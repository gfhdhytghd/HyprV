//! Secret Service transport encryption: `dh-ietf1024-sha256-aes128-cbc-pkcs7`,
//! as negotiated by libsecret and served by gnome-keyring and KWallet's
//! Secret Service. 1024-bit MODP group (RFC 2409 §6.2, generator 2) with a
//! constant-time exponentiation, HKDF-SHA256 (no salt, no info) to a 128-bit
//! key, AES-128-CBC with PKCS#7 padding and a random IV per secret. Every
//! intermediate (private exponent, shared secret, key, plaintext) is zeroized.
use aes::{
    cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit},
    Aes128,
};
use crypto_bigint::{
    const_residue, impl_modulus, modular::constant_mod::ResidueParams, Encoding, U1024,
};
use zeroize::{Zeroize, Zeroizing};

pub const ALGORITHM: &str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";
impl_modulus!(
    Oakley1024,
    U1024,
    "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7EDEE386BFB5A899FA5AE9F24117C4B1FE649286651ECE65381FFFFFFFFFFFFFFFF"
);
const PRIME_BYTES: usize = 128;

pub fn random_bytes(buf: &mut [u8]) {
    let mut filled = 0;
    while filled < buf.len() {
        let n =
            unsafe { libc::getrandom(buf[filled..].as_mut_ptr().cast(), buf.len() - filled, 0) };
        if n > 0 {
            filled += n as usize;
        }
    }
}
fn uint_from_be(bytes: &[u8]) -> Option<U1024> {
    if bytes.len() > PRIME_BYTES {
        return None;
    }
    let mut padded = Zeroizing::new([0u8; PRIME_BYTES]);
    padded[PRIME_BYTES - bytes.len()..].copy_from_slice(bytes);
    Some(U1024::from_be_slice(&padded[..]))
}
/// Unsigned big-endian without leading zeros (libsecret/gcrypt `FMT_USG`).
fn minimal_be(x: &U1024) -> Vec<u8> {
    let b = x.to_be_bytes();
    let start = b.iter().position(|v| *v != 0).unwrap_or(b.len() - 1);
    b[start..].to_vec()
}
fn pow(base: &U1024, exp: &U1024) -> U1024 {
    let r = const_residue!(base, Oakley1024);
    r.pow(exp).retrieve()
}

/// One side of a key exchange.
pub struct KeyPair {
    private: Zeroizing<U1024>,
    pub public: Vec<u8>,
}
impl KeyPair {
    pub fn generate() -> Self {
        let mut raw = Zeroizing::new([0u8; PRIME_BYTES]);
        random_bytes(&mut raw[..]);
        let private = Zeroizing::new(U1024::from_be_slice(&raw[..]));
        let public = minimal_be(&pow(&U1024::from_u8(2), &private));
        Self { private, public }
    }
    /// Derive the AES key from the peer's public value. Rejects degenerate
    /// peer values (0, 1, p-1, ≥p) that would force a known shared secret.
    pub fn derive(&self, peer_public: &[u8]) -> Option<Zeroizing<[u8; 16]>> {
        let peer = uint_from_be(peer_public)?;
        let p = <Oakley1024 as ResidueParams<{ U1024::LIMBS }>>::MODULUS;
        let p_minus_1 = p.wrapping_sub(&U1024::ONE);
        if peer <= U1024::ONE || peer >= p_minus_1 {
            return None;
        }
        let mut shared = pow(&peer, &self.private);
        let mut ikm = Zeroizing::new(shared.to_be_bytes());
        shared.zeroize();
        let mut key = Zeroizing::new([0u8; 16]);
        hkdf::Hkdf::<sha2::Sha256>::new(None, &ikm[..])
            .expand(&[], &mut key[..])
            .ok()?;
        ikm.zeroize();
        Some(key)
    }
}

/// AES-128-CBC/PKCS#7 encryption with a fresh random IV: `(iv, ciphertext)`.
pub fn encrypt(key: &[u8; 16], plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut iv = [0u8; 16];
    random_bytes(&mut iv);
    let pad = 16 - plaintext.len() % 16;
    let mut buf = Zeroizing::new(Vec::with_capacity(plaintext.len() + pad));
    buf.extend_from_slice(plaintext);
    buf.extend(std::iter::repeat_n(pad as u8, pad));
    let mut prev = iv;
    let mut out = Vec::with_capacity(buf.len());
    for chunk in buf.chunks(16) {
        let mut block = GenericArray::clone_from_slice(chunk);
        for (b, p) in block.iter_mut().zip(prev.iter()) {
            *b ^= p;
        }
        cipher.encrypt_block(&mut block);
        prev.copy_from_slice(&block);
        out.extend_from_slice(&block);
    }
    (iv.to_vec(), out)
}
/// Inverse of [`encrypt`]; `None` on bad IV length, block size or padding.
pub fn decrypt(key: &[u8; 16], iv: &[u8], ciphertext: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    if iv.len() != 16 || ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return None;
    }
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut prev = [0u8; 16];
    prev.copy_from_slice(iv);
    let mut out = Zeroizing::new(Vec::with_capacity(ciphertext.len()));
    for chunk in ciphertext.chunks(16) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        for (b, p) in block.iter_mut().zip(prev.iter()) {
            *b ^= p;
        }
        prev.copy_from_slice(chunk);
        out.extend_from_slice(&block);
        block.zeroize();
    }
    let pad = *out.last()? as usize;
    if pad == 0
        || pad > 16
        || out.len() < pad
        || !out[out.len() - pad..].iter().all(|b| *b as usize == pad)
    {
        return None;
    }
    let keep = out.len() - pad;
    out.truncate(keep);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_sides_derive_the_same_key_and_roundtrip() {
        let (a, b) = (KeyPair::generate(), KeyPair::generate());
        let (ka, kb) = (a.derive(&b.public).unwrap(), b.derive(&a.public).unwrap());
        assert_eq!(*ka, *kb);
        for msg in [
            &b""[..],
            b"x",
            b"exactly16bytes!!",
            b"a longer secret that spans blocks",
        ] {
            let (iv, ct) = encrypt(&ka, msg);
            assert_eq!(ct.len() % 16, 0);
            if !msg.is_empty() {
                assert_ne!(
                    &ct[..msg.len().min(ct.len())],
                    msg,
                    "ciphertext is not plaintext"
                );
            }
            assert_eq!(&decrypt(&kb, &iv, &ct).unwrap()[..], msg);
        }
        assert!(a.derive(&[1]).is_none(), "degenerate peer value rejected");
        assert!(a.derive(&[0xff; 128]).is_none());
    }
    #[test]
    fn aes_cbc_matches_nist_sp800_38a() {
        // SP 800-38A F.2.1 CBC-AES128.Encrypt, first block (no padding involved).
        let key: [u8; 16] = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
        let iv = hex("000102030405060708090a0b0c0d0e0f");
        let pt = hex("6bc1bee22e409f96e93d7e117393172a");
        let cipher = Aes128::new(GenericArray::from_slice(&key));
        let mut block = GenericArray::clone_from_slice(&pt);
        for (b, p) in block.iter_mut().zip(iv.iter()) {
            *b ^= p;
        }
        cipher.encrypt_block(&mut block);
        assert_eq!(block.to_vec(), hex("7649abac8119b246cee98e9b12e9197d"));
        // And our decrypt inverts it once PKCS#7 padding is appended.
        let mut padded = pt.clone();
        padded.extend([16u8; 16]);
        let mut ct = block.to_vec();
        let mut b2 = GenericArray::clone_from_slice(&padded[16..]);
        for (b, p) in b2.iter_mut().zip(block.iter()) {
            *b ^= p;
        }
        cipher.encrypt_block(&mut b2);
        ct.extend_from_slice(&b2);
        assert_eq!(&decrypt(&key, &iv, &ct).unwrap()[..], &pt[..]);
    }
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
