/*
 * Linker script for creating RISC-V qemu binary.
 * vim:ft=ld
 */

OUTPUT_ARCH(riscv64gc)

ENTRY(_boot)

SECTIONS
{
  . = 0;

  .text : {
    PROVIDE(__kernel_start = .);

    PROVIDE(__text_start = .);
    *(.text.init)
    *(.text)
    *(.text.*)
    . = ALIGN(0x1000);
    PROVIDE(__text_end = .);
  }

  .rodata : {
    PROVIDE(__rodata_start = .);
    *(.rodata)
    *(.rodata.*)
    . = ALIGN(0x1000);
    PROVIDE(__rodata_end = .);
  }

  /* put global pointer into the middle of the data sectin */
  PROVIDE(__global_pointer$ = . + 0x800);

  .data : {
    . = ALIGN(0x1000);
    PROVIDE(__data_start = .);
    *(.sdata)
    *(.sdata.*)

    *(.data)
    *(.data.*)
    . = ALIGN(0x1000);
    PROVIDE(__data_end = .);
  }

  .tdata : {
    PROVIDE(__tdata_start = .);
    *(.tdata)
    *(.tdata.*)
    . = ALIGN(0x1000);
    PROVIDE(__tdata_end = .);
  }

  .dynsym : {
		PROVIDE(__dyn_sym_start = .);
		*(.dynsym)
		PROVIDE(__dyn_sym_end = .);
	}

	.rela.dyn : {
		PROVIDE(__rel_dyn_start = .);
		*(.rela*)
		. = ALIGN(8);
		PROVIDE(__rel_dyn_end = .);
	}

  . = ALIGN(0x1000);

  .bss : {
    PROVIDE(__bss_start = .);
    *(.sbss)
    *(.sbss.*)
    *(.bss)
    *(.bss.*)
    . = ALIGN(0x1000);
    PROVIDE(__bss_end = .);
  }

  /* The stack is only 8K large, because we switch to a new stack before running the kernel */
  PROVIDE(__stack_start = .);
  . += 0x2000;
  PROVIDE(__stack_end = .);

  PROVIDE(__kernel_end = ALIGN(0x1000));

  /DISCARD/ : { *(.eh_frame_hdr .eh_frame) }
}
