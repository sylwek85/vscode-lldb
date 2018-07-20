use handles::{Handle, HandleTree};
use lldb::*;

pub struct AddressSpace {
    target: SBTarget,
    by_handle: HandleTree,
    by_address: Vec<DisassembledRange>,
}

impl AddressSpace {
    pub fn new(target: &SBTarget) {
        AddressSpace {
            target: target.clone(),
            by_handle: HandleTree,
            by_address: vec![],
        }
    }

    const NO_SYMBOL_INSTRUCTIONS: u32 = 32;

    pub fn create_from_address(addr: &SBAddress) -> DisassembledRange {
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
                start_addr = addr;
                instructions = self.target.read_instructions(start_addr,NO_SYMBOL_INSTRUCTIONS + 1);
                let last_instr = instructions.instruction_at_index(instructions.len()-1);
                end_addr = last_instr.address();
            }
        }
    }
}

struct DisassembledRange {
    start_sbaddr: SBAddress,
    start_address: u64,
    end_address: u64,
    target: SBTarget,
    source_ref: Handle,
}

impl DisassembledRange {
    const MAX_INSTR_BYTES: u32 = 8;

    fn new(target: &SBTarget, start_sbaddr: SBAddress, end_sbaddr: SBAddress, instructions: SBInstructionList) {
        let start_address = start_sbaddr.load_address(target);
        let end_address = end_sbaddr.load_address(target);
        DisassembledRange {
            target: target.clone(),
            start_sbaddr,
            end_sbaddr,
            start_address,
            end_address,
            instructions
        }
    }
}
