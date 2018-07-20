use std::borrow::Cow;
use std::rc::Rc;
use std::str;
use std::collections::BTreeMap;

use handles::{Handle, HandleTree};
use lldb::*;

pub struct AddressSpace {
    target: SBTarget,
    by_handle: HandleTree<Rc<DisassembledRange>>,
    by_address: Vec<Rc<DisassembledRange>>,
}

impl AddressSpace {
    pub fn new(target: &SBTarget) -> AddressSpace {
        AddressSpace {
            target: target.clone(),
            by_handle: HandleTree::new(),
            by_address: vec![],
        }
    }

    const NO_SYMBOL_INSTRUCTIONS: u32 = 32;

    pub fn create_from_address(&self, addr: &SBAddress) /*-> DisassembledRange*/
    {
        let start_addr;
        let end_addr;
        let instructions;
        match addr.symbol() {
            Some(symbol) => {
                start_addr = symbol.start_address();
                end_addr = symbol.end_address();
                instructions = symbol.instructions(&self.target);
            }
            None => {
                start_addr = addr.clone();
                instructions = self
                    .target
                    .read_instructions(&start_addr, AddressSpace::NO_SYMBOL_INSTRUCTIONS + 1);
                let last_instr = instructions.instruction_at_index((instructions.len() - 1) as u32);
                end_addr = last_instr.address();
            }
        }
        let dasm_range = DisassembledRange::new(&self.target, start_addr, end_addr, instructions);
    }
}

struct DisassembledRange {
    start_sbaddr: SBAddress,
    end_sbaddr: SBAddress,
    start_address: u64,
    end_address: u64,
    target: SBTarget,
    source_ref: Handle,
}

impl DisassembledRange {
    const MAX_INSTR_BYTES: u32 = 8;

    fn new(
        target: &SBTarget, start_sbaddr: SBAddress, end_sbaddr: SBAddress, instructions: SBInstructionList,
    ) -> DisassembledRange {
        let start_address = start_sbaddr.load_address(target);
        let end_address = end_sbaddr.load_address(target);
        DisassembledRange {
            target: target.clone(),
            start_sbaddr,
            end_sbaddr,
            start_address,
            end_address,
            source_ref: Handle::new(0).unwrap(),
        }
    }

    fn get_source_text(&self) {
        let source_location: Cow<str> = match self.start_sbaddr.line_entry() {
            Some(le) => format!("{}:{}", le.file_spec().path(), le.line()).into(),
            None => "unknown".into(),
        };
        let description: Cow<str> = match self.start_sbaddr.symbol() {
            Some(symbol) => {
                let mut descr = SBStream::new();
                if symbol.get_description(&mut descr) {
                    match str::from_utf8(descr.data()) {
                        Ok(s) => Some(s.to_owned().into()),
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            None => None,
        }.unwrap_or("No Symbol Info".into());

        unimplemented!()

        // lines = [
        //     '; %s' % description,
        //     '; Source location: %s' % source_location ]
        // dump = []
        // for instr in self.instructions:
        //     addr = instr.GetAddress().GetLoadAddress(self.target)
        //     del dump[:]
        //     for i,b in enumerate(instr.GetData(self.target).uint8):
        //         if i >= MAX_INSTR_BYTES:
        //             dump.append('>')
        //             break
        //         dump.append('%02X ' % b)
        //     comment = instr.GetComment(self.target)
        //     line = '%08X: %s %-6s %s%s%s' % (
        //         addr,
        //         ''.join(dump).ljust(MAX_INSTR_BYTES * 3 + 2),
        //         instr.GetMnemonic(self.target),
        //         instr.GetOperands(self.target),
        //         '  ; ' if len(comment) > 0 else '',
        //         comment
        //     )
        //     lines.append(line)
        // return '\n'.join(lines)
    }
}
