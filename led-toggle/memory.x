flash_start       = 0x08000000;
total_flash       = 256K;
header_length     = 256;
bootloader_length = 0K;
bank_length       = 256K;
bank_offset       = 0 * bank_length + bootloader_length;
program_start     = flash_start + bank_offset;
program_length    = bank_length - header_length;
header_start      = program_start + program_length;

ram_start  = 0x20000000;
ram_length = 144K;

MEMORY
{
  FLASH    : ORIGIN = program_start, LENGTH = program_length
  HEADER   : ORIGIN = header_start,  LENGTH = header_length
  RAM(rwx) : ORIGIN = ram_start,    LENGTH = ram_length
}
