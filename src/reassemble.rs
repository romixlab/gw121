//! Reassembles 121GW packets from a stream of BLE indications.
//!
//! The BLE122 module forwards the meter's UART bytes verbatim, one ATT indication per up-to-20-byte chunk
//! (the default BLE ATT MTU minus overhead), with no relationship between indication boundaries and the
//! firmware's 19-byte packet boundaries: a packet can straddle two indications, and an indication can hold
//! parts of two packets. BLE communication is also reported to be flaky (dropped or truncated
//! indications), so this must resynchronize on the next `0xF2` rather than assume alignment
//! (<https://sigrok.org/wiki/EEVBlog_121GW>, "Protocol" > "BLE").

use crate::protocol::{self, PACKET_SIZE};

/// Buffers incoming bytes and yields validated, 19-byte-aligned packets.
#[derive(Debug, Default)]
pub struct Reassembler {
    buf: Vec<u8>,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues bytes from one BLE indication.
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// Returns the next validated packet, if the buffer holds one. Drops bytes before a packet it resyncs
    /// past (a dropped byte, a corrupt packet, or genuine noise); call repeatedly after each [`push`] until it
    /// returns `None`.
    pub fn next_packet(&mut self) -> Option<[u8; PACKET_SIZE]> {
        loop {
            let start = self.buf.iter().position(|&b| b == protocol::START_BYTE)?;
            if self.buf.len() < start + PACKET_SIZE {
                // Not enough buffered yet for a full packet at this start byte; drop what precedes it (known
                // garbage) and wait for more.
                self.buf.drain(..start);
                return None;
            }
            let mut candidate = [0u8; PACKET_SIZE];
            candidate.copy_from_slice(&self.buf[start..start + PACKET_SIZE]);
            if protocol::validate(&candidate).is_ok() {
                self.buf.drain(..start + PACKET_SIZE);
                return Some(candidate);
            }
            // Not a real packet start (checksum or a reserved bit disagreed): it was data, not a frame marker.
            // Drop just this one byte and keep scanning from the next 0xF2.
            self.buf.drain(..=start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    /// The exact indications from the sigrok wiki's BLE capture
    /// (<https://sigrok.org/wiki/EEVBlog_121GW>, "Protocol" > "BLE"), none aligned to a 19-byte packet.
    const IND1: &str = "f2 17 80 01 21 01 40 00 15 64 01 00 f0 00 00 0e 40 00 ca f2";
    const IND2: &str = "17 80 01 21 01 40 00 01 64 01 00 f0 00 00 0e 40 00 de f2 17";
    const IND3: &str = "80 01 21 01 00 00 1f 64 01 00 ef 00 00 0e 40 00 9f f2 17 80";

    #[test]
    fn reassembles_packets_split_across_indications() {
        let mut r = Reassembler::new();
        r.push(&hex(IND1));
        let p1 = r.next_packet().expect("packet 1 after indication 1");
        assert_eq!(p1[0], protocol::START_BYTE);
        assert!(
            r.next_packet().is_none(),
            "only one full packet buffered yet"
        );

        r.push(&hex(IND2));
        let p2 = r.next_packet().expect("packet 2 after indication 2");
        assert_eq!(p2[8], 0x01); // main value low byte distinguishes packet 2 from 1 and 3
        assert!(r.next_packet().is_none());

        r.push(&hex(IND3));
        let p3 = r.next_packet().expect("packet 3 after indication 3");
        assert_eq!(p3[6], 0x00); // main range byte: positive sign, unlike 1 and 2
        assert!(r.next_packet().is_none());
    }

    #[test]
    fn resyncs_past_leading_garbage() {
        let mut r = Reassembler::new();
        r.push(&[0xAA, 0xBB, 0xCC]); // noise, no 0xF2 at all yet
        assert!(r.next_packet().is_none());
        r.push(&hex(IND1));
        assert!(r.next_packet().is_some());
    }

    #[test]
    fn resyncs_past_a_corrupt_packet() {
        // A full, valid packet (from the wiki capture, see protocol::tests::PKT2) immediately follows a
        // corrupted one: a dropped byte never shows up as a spurious 0xF2 inside it, so there is no false
        // resync point to skip past first.
        const PKT2: [u8; PACKET_SIZE] = [
            0xf2, 0x17, 0x80, 0x01, 0x21, 0x01, 0x40, 0x00, 0x01, 0x64, 0x01, 0x00, 0xf0, 0x00,
            0x00, 0x0e, 0x40, 0x00, 0xde,
        ];
        let mut corrupt = hex(IND1)[..PACKET_SIZE].to_vec();
        corrupt[10] ^= 0xFF; // flip a payload byte so the checksum no longer matches
        let mut r = Reassembler::new();
        r.push(&corrupt);
        r.push(&PKT2);
        let p = r
            .next_packet()
            .expect("should resync onto the valid packet");
        assert_eq!(p, PKT2);
        protocol::validate(&p).unwrap();
    }
}
