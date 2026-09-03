//! SPKI DER for the four key types (PROTOCOL.md §1).
//!
//! Every public key VoleNet puts on the wire is `base64(SPKI DER)`, and the instance id is a
//! hash of that base64 text — so the bytes have to be exactly what OpenSSL emits. The shape is
//! fixed: `SEQUENCE { SEQUENCE { OID }, BIT STRING { 0x00 ‖ raw key } }`, no parameters. A
//! purpose-built encoder is smaller and more auditable than a general ASN.1 dependency.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
	Ed25519,
	X25519,
	MlKem768,
	MlDsa65,
}

impl KeyKind {
	/// DER-encoded OID, including the 0x06 tag and length.
	pub fn oid(self) -> &'static [u8] {
		match self {
			KeyKind::Ed25519 => &[0x06, 0x03, 0x2b, 0x65, 0x70], // 1.3.101.112
			KeyKind::X25519 => &[0x06, 0x03, 0x2b, 0x65, 0x6e],  // 1.3.101.110
			KeyKind::MlKem768 => &[0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x04, 0x02], // 2.16.840.1.101.3.4.4.2
			KeyKind::MlDsa65 => &[0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x03, 0x12], // 2.16.840.1.101.3.4.3.18
		}
	}

	pub fn raw_len(self) -> usize {
		match self {
			KeyKind::Ed25519 | KeyKind::X25519 => 32,
			KeyKind::MlKem768 => 1184,
			KeyKind::MlDsa65 => 1952,
		}
	}

	fn from_oid(oid: &[u8]) -> Option<KeyKind> {
		[KeyKind::Ed25519, KeyKind::X25519, KeyKind::MlKem768, KeyKind::MlDsa65]
			.into_iter()
			.find(|k| k.oid() == oid)
	}
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DerError {
	#[error("not an SPKI structure")]
	Shape,
	#[error("unknown key algorithm")]
	UnknownAlgorithm,
	#[error("key length {0} does not match the algorithm")]
	Length(usize),
}

fn push_len(out: &mut Vec<u8>, len: usize) {
	if len < 0x80 {
		out.push(len as u8);
	} else {
		let bytes = len.to_be_bytes();
		let first = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
		let sig = &bytes[first..];
		out.push(0x80 | sig.len() as u8);
		out.extend_from_slice(sig);
	}
}

fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
	let mut out = Vec::with_capacity(body.len() + 4);
	out.push(tag);
	push_len(&mut out, body.len());
	out.extend_from_slice(body);
	out
}

/// Wrap a raw public key as SPKI DER.
pub fn encode_spki(kind: KeyKind, raw: &[u8]) -> Result<Vec<u8>, DerError> {
	if raw.len() != kind.raw_len() {
		return Err(DerError::Length(raw.len()));
	}
	let alg = tlv(0x30, kind.oid());
	let mut bits = Vec::with_capacity(raw.len() + 1);
	bits.push(0x00); // no unused bits
	bits.extend_from_slice(raw);
	let bit_string = tlv(0x03, &bits);
	let mut body = alg;
	body.extend_from_slice(&bit_string);
	Ok(tlv(0x30, &body))
}

/// Read one TLV at `pos`; returns (tag, body, next position).
fn read_tlv(der: &[u8], pos: usize) -> Result<(u8, &[u8], usize), DerError> {
	let tag = *der.get(pos).ok_or(DerError::Shape)?;
	let first = *der.get(pos + 1).ok_or(DerError::Shape)?;
	let (len, start) = if first < 0x80 {
		(first as usize, pos + 2)
	} else {
		let n = (first & 0x7f) as usize;
		if n == 0 || n > 4 {
			return Err(DerError::Shape);
		}
		let mut len = 0usize;
		for i in 0..n {
			len = (len << 8) | *der.get(pos + 2 + i).ok_or(DerError::Shape)? as usize;
		}
		(len, pos + 2 + n)
	};
	let end = start.checked_add(len).ok_or(DerError::Shape)?;
	let body = der.get(start..end).ok_or(DerError::Shape)?;
	Ok((tag, body, end))
}

/// Unwrap SPKI DER into (algorithm, raw key).
pub fn decode_spki(der: &[u8]) -> Result<(KeyKind, Vec<u8>), DerError> {
	let (tag, outer, end) = read_tlv(der, 0)?;
	if tag != 0x30 || end != der.len() {
		return Err(DerError::Shape);
	}
	let (atag, alg, after_alg) = read_tlv(outer, 0)?;
	if atag != 0x30 {
		return Err(DerError::Shape);
	}
	let (otag, _oid_body, oid_end) = read_tlv(alg, 0)?;
	if otag != 0x06 {
		return Err(DerError::Shape);
	}
	let kind = KeyKind::from_oid(&alg[..oid_end]).ok_or(DerError::UnknownAlgorithm)?;
	let (btag, bits, bits_end) = read_tlv(outer, after_alg)?;
	if btag != 0x03 || bits_end != outer.len() || bits.first() != Some(&0x00) {
		return Err(DerError::Shape);
	}
	let raw = &bits[1..];
	if raw.len() != kind.raw_len() {
		return Err(DerError::Length(raw.len()));
	}
	Ok((kind, raw.to_vec()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ed25519_spki_is_the_well_known_44_byte_form() {
		let raw = [7u8; 32];
		let der = encode_spki(KeyKind::Ed25519, &raw).unwrap();
		assert_eq!(der.len(), 44);
		assert_eq!(&der[..12], &[0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00]);
		let (kind, back) = decode_spki(&der).unwrap();
		assert_eq!(kind, KeyKind::Ed25519);
		assert_eq!(back, raw);
	}

	#[test]
	fn long_form_lengths_round_trip() {
		for kind in [KeyKind::MlKem768, KeyKind::MlDsa65] {
			let raw = vec![0xabu8; kind.raw_len()];
			let der = encode_spki(kind, &raw).unwrap();
			let (k, back) = decode_spki(&der).unwrap();
			assert_eq!(k, kind);
			assert_eq!(back, raw);
		}
	}

	#[test]
	fn wrong_length_is_refused() {
		assert_eq!(encode_spki(KeyKind::X25519, &[0u8; 31]), Err(DerError::Length(31)));
	}
}
