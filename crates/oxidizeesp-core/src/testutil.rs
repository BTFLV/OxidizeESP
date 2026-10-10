//! Test helpers.

use object::{
    Endianness,
    elf,
    write::elf::{FileHeader, ProgramHeader, SectionHeader, Writer},
};

/// Build a minimal RISC-V ELF executable with one code section placed in
/// the ESP32-C3 flash-mapped instruction address range.
pub fn synthetic_esp32c3_elf(code_len: usize) -> Vec<u8> {
    const TEXT_ADDR: u64 = 0x4200_0020;
    let text: Vec<u8> = (0..code_len).map(|i| (i % 251) as u8).collect();

    let mut buffer = Vec::new();
    let mut writer = Writer::new(Endianness::Little, false, &mut buffer);
    writer.reserve_file_header();
    writer.reserve_program_headers(1);
    let text_name = writer.add_section_name(b".text");
    writer.reserve_null_section_index();
    let _text_index = writer.reserve_section_index();
    let text_offset = writer.reserve(text.len(), 4);
    writer.reserve_shstrtab_section_index();
    writer.reserve_shstrtab();
    writer.reserve_section_headers();

    writer
        .write_file_header(&FileHeader {
            os_abi: 0,
            abi_version: 0,
            e_type: elf::ET_EXEC,
            e_machine: elf::EM_RISCV,
            e_entry: TEXT_ADDR,
            e_flags: 0,
        })
        .expect("file header");
    writer.write_program_header(&ProgramHeader {
        p_type: elf::PT_LOAD,
        p_flags: elf::PF_R | elf::PF_X,
        p_offset: text_offset as u64,
        p_vaddr: TEXT_ADDR,
        p_paddr: TEXT_ADDR,
        p_filesz: text.len() as u64,
        p_memsz: text.len() as u64,
        p_align: 4,
    });
    writer.pad_until(text_offset);
    writer.write(&text);
    writer.write_shstrtab();
    writer.write_null_section_header();
    writer.write_section_header(&SectionHeader {
        name: Some(text_name),
        sh_type: elf::SHT_PROGBITS,
        sh_flags: u64::from(elf::SHF_ALLOC | elf::SHF_EXECINSTR),
        sh_addr: TEXT_ADDR,
        sh_offset: text_offset as u64,
        sh_size: text.len() as u64,
        sh_link: 0,
        sh_info: 0,
        sh_addralign: 4,
        sh_entsize: 0,
    });
    writer.write_shstrtab_section_header();
    buffer
}
