/*
 * Linker script for creating RISC-V qemu binary.
 * vim:ft=ld
 */

OUTPUT_ARCH(riscv64gc)

ENTRY(_boot)

PHDRS
{
  text PT_LOAD FLAGS(5);
  rodata PT_LOAD FLAGS(4);
  data PT_LOAD FLAGS(6);
  bss PT_LOAD FLAGS(6);
  dynamic PT_DYNAMIC;
}

SECTIONS
{
  . = 0x80200000;
  PROVIDE(__kernel_start = .);

  PROVIDE(__text_start = .);
  /* Application code */
  .text : {
    *(.text.init)
    *(.text)
    *(.text.*)
  } :text

  . = ALIGN(0x1000);
  PROVIDE(__text_end = .);

  /* All read only sections will come here to make mapping easier */
  PROVIDE(__rodata_start = .);

  /* All the symbols needed for relocation lookup */
  .hash     : { *(.hash) } :rodata
  .gnu.hash : { *(.gnu.hash) } :rodata
  .dynsym   : { *(.dynsym .dynsym.*) } :rodata
  .dynstr   : { *(.dynstr .dynstr.*) } :rodata

  /* All relocations sections */
  .rel.init       : { *(.rel.init) } :rodata
  .rela.init      : { *(.rela.init) } :rodata
  .rel.text       : { *(.rel.text .rel.text.* .rel.gnu.linkonce.t.*) } :rodata
  .rela.text      : { *(.rela.text .rela.text.* .rela.gnu.linkonce.t.*) } :rodata
  .rel.fini       : { *(.rel.fini) } :rodata
  .rela.fini      : { *(.rela.fini) } :rodata
  .rel.rodata     : { *(.rel.rodata .rel.rodata.* .rel.gnu.linkonce.r.*) } :rodata
  .rela.rodata    : { *(.rela.rodata .rela.rodata.* .rela.gnu.linkonce.r.*) } :rodata
  .rel.data       : { *(.rel.data .rel.data.* .rel.gnu.linkonce.d.*) } :rodata
  .rela.data      : { *(.rela.data .rela.data.* .rela.gnu.linkonce.d.*) } :rodata
  .rel.tdata	    : { *(.rel.tdata .rel.tdata.* .rel.gnu.linkonce.td.*) } :rodata
  .rela.tdata	    : { *(.rela.tdata .rela.tdata.* .rela.gnu.linkonce.td.*) } :rodata
  .rel.tbss	      : { *(.rel.tbss .rel.tbss.* .rel.gnu.linkonce.tb.*) } :rodata
  .rela.tbss	    : { *(.rela.tbss .rela.tbss.* .rela.gnu.linkonce.tb.*) } :rodata
  .rel.ctors      : { *(.rel.ctors) } :rodata
  .rela.ctors     : { *(.rela.ctors) } :rodata
  .rel.dtors      : { *(.rel.dtors) } :rodata
  .rela.dtors     : { *(.rela.dtors) } :rodata
  .rel.got        : { *(.rel.got) } :rodata
  .rela.got       : { *(.rela.got) } :rodata
  .rel.bss        : { *(.rel.bss .rel.bss.* .rel.gnu.linkonce.b.*) } :rodata
  .rela.bss       : { *(.rela.bss .rela.bss.* .rela.gnu.linkonce.b.*) } :rodata
  .rel.plt        : { *(.rel.plt) } :rodata
  .rela.plt       : { *(.rela.plt) } :rodata

  .rodata : {
    *(.rodata)
    *(.rodata.*)
  }: rodata

  /* TODO: eh_frame with proper exception handling */

  . = ALIGN(0x1000);
  PROVIDE(__rodata_end = .);

  /* Put global pointer between rodata and data */
  PROVIDE(__global_pointer$ = .);

  /* Start of read write sections */
  . = ALIGN(0x1000);
  PROVIDE(__data_start = .);

  .data : {
    *(.sdata)
    *(.sdata.*)

    *(.data)
    *(.data.*)
  } :data

  /* All GOT sections */
  .got : { *(.got.plt) *(.igot.plt) *(.got) *(.igot) } :data

  /* The dynamic section as we need it to be stored in the binary */
  .dynamic : {
    PROVIDE(__dynamic_start = .);
    *(.dynamic)
  } :data :dynamic

  /* Thread Local sections */
  .tdata : {
    PROVIDE(__tdata_start = .);
    *(.tdata)
    *(.tdata.*)
    PROVIDE(__tdata_end = .);
  } :data

  .tbss : {
    *(.tbss .tbss.*)
    *(.tcommon)
  } :data

  . = ALIGN(0x1000);
  PROVIDE(__data_end = .);

  /* BSS section is separate for zeroing it out */

  PROVIDE(__bss_start = .);
  .bss : {
    *(.sbss)
    *(.sbss.*)
    *(.bss)
    *(.bss.*)
  } :bss
  . = ALIGN(0x1000);
  PROVIDE(__bss_end = .);

  /* The stack is only 8K large, because we switch to a new stack before running the kernel */
  PROVIDE(__stack_start = .);
  . += 0x2000;
  PROVIDE(__stack_end = .);

  PROVIDE(__kernel_end = ALIGN(0x1000));

  /DISCARD/ : { *(.eh_frame_hdr .eh_frame) }
}
