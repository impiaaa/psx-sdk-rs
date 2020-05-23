use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use Section;
use SectionType;
use Region;

pub struct PsxWriter {
    psexe:  File,
    region: Region,
}

impl PsxWriter {
    pub fn new(path: &Path, region: Region) -> PsxWriter {
        let psexe =
            match OpenOptions::new()
            .write(true).create(true).truncate(true).open(path) {
                Ok(psexe) => psexe,
                Err(e) => panic!("Can't open {}: {}", path.display(), e),
            };

        PsxWriter {
            psexe: psexe,
            region: region,
        }
    }

    pub fn dump(mut self, entry: u32, mut sections: Vec<Section>, gp: u32) {
        // Magic
        self.write(b"PS-X EXE");

        // Padding
        self.write(&[0; 8]);

        // First PC address (entry point)
        println!("Entry PC:       0x{:08x}", entry);
        self.write32(entry);

        // Initial GP
        println!("Initial GP:     0x{:08x}", gp);
        self.write32(gp);

        // Sort the sections by base address since that's how we're
        // going to dump them
        sections.sort_by(|s1, s2| s1.base.cmp(&s2.base));

        // Base address
        let base = sections[0].base;
        println!("Base address:   0x{:08x}", base);
        self.write32(base);

        // Object size (file size minus the 2048bytes header). Since
        // we've sorted the list by base address and sections
        // shouldn't overlap we just look for the lats progbit section
        // and see where it ends. Then we can just substract the base
        // address
        let end_addr = sections.iter().filter_map(
            |s| {
                match s.contents {
                    // For progbit sections we compute the end address
                    // and return that
                    SectionType::ProgBits(ref p) =>
                        Some(s.base + p.len() as u32),
                    // We ignore memfill sections since they take no
                    // space in the file
                    SectionType::Memfill(_) => None,
                }
            })
            // We only care about the last section
            .last();

        let end_addr =
            match end_addr {
                Some(e) => e,
                _ => panic!("No progbits section found!"),
            };

        let actual_object_size = end_addr - base;
        // Arbitrarily refuse object files greater than 1MB. The PSX
        // only has 2MB of RAM, most executables are a few hundred KBs
        // at most.
        if actual_object_size > 1 * 1024 * 1024 {
            panic!("Object is too big");
        }
        
        let padded_object_size = if (actual_object_size % 2048) == 0 {
            actual_object_size
        } else {
            actual_object_size + 2048 - (actual_object_size % 2048)
        };

        println!(
            "Text+data size: {}B (actual {}B)",
            padded_object_size, actual_object_size
        );
        self.write32(padded_object_size);

        // I don't know what the two next words do but the Nocash spec
        // says that they're "usually 0"
        self.write(&[0; 8]);

        // Next we want to initialize the memfill
        let memfill = sections.iter().filter_map(
            |s| {
                match s.contents {
                    SectionType::Memfill(len) => Some((s.base, len)),
                    _ => None,
                }
            });

        let (memfill_base, memfill_length) =
            memfill.fold((0, 0), |(base, lensum), (secbase, seclen)| {
                if base == 0 {
                    (secbase, seclen)
                } else if base+lensum == secbase {
                    (base, lensum+seclen)
                } else {
                    panic!("Got discontiguous memfill sections!");
                }
            });

        println!("Memfill base:   0x{:08x}", memfill_base);
        self.write32(memfill_base);
        println!("Memfill length: {}B", memfill_length);
        self.write32(memfill_length);

        // For now hardcode SP base and offset.
        let sp     = 0x801ffff0;
        let sp_off = 0;

        println!("SP base:        0x{:08x}", sp);
        self.write32(sp);
        println!("SP offset:      {}", sp_off);
        self.write32(sp_off);

        // Padding that is used by the BIOS to store R16, R28, R30, SP
        // and RA when it starts the execution of our program.
        self.write(&[0; 20]);

        // License marker.
        self.write(b"Sony Computer Entertainment Inc. for ");

        let region_str =
            match self.region {
                Region::NorthAmerica => "North America area",
                Region::Europe       => "Europe area",
                Region::Japan        => "Japan area",
            };

        println!("Region:         {}", region_str);
        self.write(region_str.as_bytes());

        // *huge* pad before we reach the actual object. Not sure why
        // they did that...
        let pad = vec![0; 1935 - region_str.len()];
        self.write(&pad);

        // Finally we can dump the progbits sections
        let progbits = sections.iter().filter_map(
            |s| {
                match &s.contents {
                    &SectionType::ProgBits(ref data) => Some((s.base, data)),
                    _ => None,
                }
            });

        let mut offset = base;

        for (base, data) in progbits {
            // If there's a gap between the previous section and this
            // one we fill it with 0s
            let padlen = base - offset;
            let pad = vec![0; padlen as usize];
            self.write(&pad);

            // And we can dump the data
            self.write(data);

            // Update the offset
            offset = base + data.len() as u32;
        }
        
        let endpad = vec![0; (padded_object_size - actual_object_size) as usize];
        self.write(&endpad);
    }

    fn write(&mut self, v: &[u8]) {
        match self.psexe.write(v) {
            Ok(n) => {
                if n != v.len() {
                    panic!("Couldn't write {} bytes to file", v.len());
                }
            }
            Err(e) => panic!("Write failed: {}", e),
        }
    }

    /// Write 32bit value in the file in little endian
    fn write32(&mut self, v: u32) {
        self.write(&[ v as u8,
                      (v >> 8) as u8,
                      (v >> 16) as u8,
                      (v >> 24) as u8]);
    }
}
