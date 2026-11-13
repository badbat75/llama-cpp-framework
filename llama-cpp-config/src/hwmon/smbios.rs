//! The firmware's SMBIOS table, as Windows hands it to any process
//! (`GetSystemFirmwareTable`): the memory modules (type 17) and the processor's
//! socket and clocks (type 4). The offsets are those of the DMTF SMBIOS
//! specification. Only what is read here is parsed, and a field past the end of
//! a structure (the firmware's SMBIOS is older than the field) stays empty.

use crate::devices::MemoryModule;

const END_OF_TABLE: u8 = 127;
const PROCESSOR: u8 = 4;
const MEMORY_DEVICE: u8 = 17;

/// A structure's formatted area and the strings after it.
struct Structure<'a> {
    kind: u8,
    formatted: &'a [u8],
    strings: Vec<&'a [u8]>,
}

impl Structure<'_> {
    fn u8_at(&self, at: usize) -> Option<u8> {
        self.formatted.get(at).copied()
    }

    fn u16_at(&self, at: usize) -> Option<u16> {
        let b = self.formatted.get(at..at + 2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32_at(&self, at: usize) -> Option<u32> {
        let b = self.formatted.get(at..at + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The string a field numbers, from 1; empty for none and for the
    /// placeholders firmware writes in a field it did not fill.
    fn text_at(&self, at: usize) -> String {
        let text = self
            .u8_at(at)
            .and_then(|n| self.strings.get((n as usize).checked_sub(1)?))
            .map(|s| String::from_utf8_lossy(s).trim().to_string())
            .unwrap_or_default();
        const PLACEHOLDERS: [&str; 6] = [
            "unknown",
            "not specified",
            "not available",
            "none",
            "undefined",
            "to be filled by o.e.m.",
        ];
        if PLACEHOLDERS.contains(&text.to_ascii_lowercase().as_str()) {
            String::new()
        } else {
            text
        }
    }
}

fn structures(table: &[u8]) -> Vec<Structure<'_>> {
    let mut found = Vec::new();
    let mut at = 0;
    while let (Some(&kind), Some(&length)) = (table.get(at), table.get(at + 1)) {
        let length = length as usize;
        let Some(formatted) = table.get(at..at + length).filter(|_| length >= 4) else {
            break;
        };
        // The strings end in an empty one, so in two zero bytes.
        let start = at + length;
        let Some(end) = table
            .get(start..)
            .and_then(|rest| rest.windows(2).position(|w| w == [0, 0]))
        else {
            break;
        };
        let strings = table[start..start + end]
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .collect();
        if kind == END_OF_TABLE {
            break;
        }
        found.push(Structure {
            kind,
            formatted,
            strings,
        });
        at = start + end + 2;
    }
    found
}

fn memory_kind(code: u8) -> Option<&'static str> {
    Some(match code {
        0x18 => "DDR3",
        0x1A => "DDR4",
        0x1B => "LPDDR",
        0x1C => "LPDDR2",
        0x1D => "LPDDR3",
        0x1E => "LPDDR4",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        _ => return None,
    })
}

/// A module's size in bytes; `None` for an empty slot or an unknown size.
fn module_bytes(s: &Structure) -> Option<u64> {
    const MIB: u64 = 1 << 20;
    match s.u16_at(0x0C)? {
        0 | 0xFFFF => None,
        // The size did not fit: it is in the extended field, in MiB.
        0x7FFF => s.u32_at(0x1C).map(|mib| (mib & 0x7FFF_FFFF) as u64 * MIB),
        kib if kib & 0x8000 != 0 => Some((kib & 0x7FFF) as u64 * 1024),
        mib => Some(mib as u64 * MIB),
    }
}

/// A speed in MT/s, or `None` when unknown; 0xFFFF sends it to its extended
/// field.
fn speed(s: &Structure, at: usize, extended_at: usize) -> Option<u32> {
    match s.u16_at(at)? {
        0 => None,
        0xFFFF => s
            .u32_at(extended_at)
            .map(|v| v & 0x7FFF_FFFF)
            .filter(|v| *v != 0),
        mts => Some(mts as u32),
    }
}

/// The installed modules, in the firmware's order.
pub fn memory_modules(table: &[u8]) -> Vec<MemoryModule> {
    structures(table)
        .iter()
        .filter(|s| s.kind == MEMORY_DEVICE)
        .filter_map(|s| {
            let bytes = module_bytes(s).filter(|b| *b > 0)?;
            let slot: Vec<String> = [s.text_at(0x11), s.text_at(0x10)]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect();
            Some(MemoryModule {
                maker: s.text_at(0x17),
                part: s.text_at(0x1A),
                slot: slot.join(" / "),
                bytes,
                kind: s.u8_at(0x12).and_then(memory_kind).map(str::to_string),
                configured_mts: speed(s, 0x20, 0x58),
                rated_mts: speed(s, 0x15, 0x54),
                configured_millivolts: s.u16_at(0x26).filter(|mv| *mv != 0).map(u32::from),
            })
        })
        .collect()
}

/// The first processor's socket and maximum clock in MHz.
pub fn processor(table: &[u8]) -> (Option<String>, Option<u32>) {
    let all = structures(table);
    let Some(s) = all.iter().find(|s| s.kind == PROCESSOR) else {
        return (None, None);
    };
    let socket = Some(s.text_at(0x04)).filter(|t| !t.is_empty());
    (
        socket,
        s.u16_at(0x14).filter(|mhz| *mhz != 0).map(u32::from),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A structure with `fields` from offset 4 on, the rest of `length` zero.
    fn structure(kind: u8, length: u8, fields: &[(usize, &[u8])], strings: &[&str]) -> Vec<u8> {
        let mut out = vec![0u8; length as usize];
        out[0] = kind;
        out[1] = length;
        for (at, bytes) in fields {
            out[*at..*at + bytes.len()].copy_from_slice(bytes);
        }
        for s in strings {
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        if strings.is_empty() {
            out.push(0);
        }
        out.push(0);
        out
    }

    fn module(size: [u8; 2], strings: &[&str]) -> Vec<u8> {
        structure(
            MEMORY_DEVICE,
            0x28,
            &[
                (0x0C, &size),
                (0x10, &[1]),
                (0x11, &[2]),
                (0x12, &[0x22]),
                (0x15, &6400u16.to_le_bytes()),
                (0x17, &[3]),
                (0x1A, &[4]),
                (0x20, &6000u16.to_le_bytes()),
                (0x26, &1100u16.to_le_bytes()),
            ],
            strings,
        )
    }

    #[test]
    fn modules_are_read_and_empty_slots_are_left_out() {
        let mut table = structure(
            PROCESSOR,
            0x18,
            &[(0x04, &[1]), (0x14, &5600u16.to_le_bytes())],
            &["AM5"],
        );
        table.extend(module(
            [0, 0x10],
            &["DIMM 1", "P0 CHANNEL A", "CORSAIR", "CMK64 "],
        ));
        // Empty slots: a size of zero, and of zero KiB.
        table.extend(module(
            [0, 0],
            &["DIMM 2", "P0 CHANNEL A", "Unknown", "Unknown"],
        ));
        table.extend(module(
            [0, 0x80],
            &["DIMM 2", "P0 CHANNEL B", "Unknown", "Unknown"],
        ));
        table.extend(structure(END_OF_TABLE, 4, &[], &[]));
        // After the end of the table nothing counts.
        table.extend(module([0, 0x10], &["DIMM 3", "P1", "x", "y"]));
        assert_eq!(
            memory_modules(&table),
            [MemoryModule {
                maker: "CORSAIR".to_string(),
                part: "CMK64".to_string(),
                slot: "P0 CHANNEL A / DIMM 1".to_string(),
                bytes: 0x1000 << 20,
                kind: Some("DDR5".to_string()),
                configured_mts: Some(6000),
                rated_mts: Some(6400),
                configured_millivolts: Some(1100),
            }]
        );
    }

    #[test]
    fn a_size_past_16_bits_is_in_the_extended_field() {
        let mut table = structure(
            MEMORY_DEVICE,
            0x28,
            &[
                (0x0C, &0x7FFFu16.to_le_bytes()),
                (0x1C, &65536u32.to_le_bytes()),
            ],
            &[],
        );
        table.extend(structure(END_OF_TABLE, 4, &[], &[]));
        assert_eq!(memory_modules(&table)[0].bytes, 64 << 30);
    }

    #[test]
    fn placeholders_are_no_text_and_an_old_structure_has_no_late_fields() {
        // Ends before the configured speed.
        let mut table = structure(
            MEMORY_DEVICE,
            0x1B,
            &[(0x0C, &8192u16.to_le_bytes()), (0x17, &[1]), (0x1A, &[2])],
            &["Not Specified", "PART"],
        );
        table.extend(structure(END_OF_TABLE, 4, &[], &[]));
        let m = &memory_modules(&table)[0];
        assert_eq!((m.maker.as_str(), m.part.as_str()), ("", "PART"));
        assert_eq!(
            (m.configured_mts, m.rated_mts, m.configured_millivolts),
            (None, None, None)
        );
    }

    #[test]
    fn the_processor_gives_its_socket_and_maximum_clock() {
        let mut table = structure(
            PROCESSOR,
            0x18,
            &[(0x04, &[1]), (0x14, &5600u16.to_le_bytes())],
            &["AM5"],
        );
        table.extend(structure(END_OF_TABLE, 4, &[], &[]));
        assert_eq!(processor(&table), (Some("AM5".to_string()), Some(5600)));
        assert_eq!(processor(&[]), (None, None));
    }

    #[test]
    fn a_truncated_table_ends_the_walk() {
        assert!(memory_modules(&[MEMORY_DEVICE, 0x28, 0, 0]).is_empty());
        assert!(memory_modules(&[MEMORY_DEVICE]).is_empty());
    }
}
