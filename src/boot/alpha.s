; Stage One Bootloader
;
; The only goal of the stage1 bootloader is to load and execute the stage2
; bootloader. The distinction between stages is a necessity forced by the
; initial conditions. In order to begin the execution of the stage1 bootloader
; the BIOS looks for a bootable media and loads its first 512 bytes. These 512
; bytes must contain enough information to guide the rest of the loading of
; the operating system. This is complicated by the metadata necessary for a
; Master Boot Record, occupying at least 64 bytes to hold the 4 entries in the
; partition table. Historically, the first partition always begins on the 63rd
; sector, leaving some unused space to store the stage2 bootloader. The stage1
; bootloader will read the sectors following it to load the stage2 bootloader,
; which, with loosened size constraints, will contain the filesystem drivers
; necessary to find and boot a kernel from within the filesystem.

BITS 16 ; execution begins in real mode

; bootsector is loaded at 0x0000:0x7c00
; but it will relocate and execute from 0x0000:0x0600
ORG 0x0600

stage1:
	; because the volume bootloader must be read to 0x7c00
	; relocate this code to 0x0600, then jmp to it
	mov cx, 0x200
.relocate:
	mov al, [0x7c00 + ecx]
	mov [0x0600 + ecx], al
	loop .relocate

	; absolute jmp is easier than trying encode the proper relative jmp
	mov ax, .moved
	jmp ax

.moved:
	; BIOS tells us what drive we booted from, save that value
	mov [drive_number], dl

	; clear window
	mov ax, 0x0003
	int 0x10

	; check support for the proper BIOS extensions
	mov ah, 0x41
	mov bx, 0x55AA
	; dl = drive number
	int 0x13
	; CF set if unsupported
	jc unsupported_bios

	; zero some registers
	xor ax, ax
	mov ds, ax
	mov ss, ax
	mov sp, ax

	; read stage2
	mov ah, 0x42 ; read LBA
	; dl = drive number
	mov si, LBA_Packet
	int 0x13 ; BIOS mass storage
	jc exit ; TODO error handling

	mov si, .message
	call print_stringln

	jmp stage2


print_char:
	; al should have the char to print
	mov ah, 0x0E
	mov bh, 0x00
	mov bl, 0x07
	int 0x10
	ret

print_string:
.loop:
	mov al, [si]
	inc si
	call print_char
	or al, al
	jnz .loop
	ret

print_stringln:
	call print_string

	; get cursor position
	mov ah, 0x03 ; get cursor
	mov bh, 0x00 ; page 0
	int 0x10 ; BIOS video
	; dh = row, dl = col

	; move cursor to line below
	inc dh ; increment row
	xor dl, dl ; reset column
	mov ah, 0x02 ; set cursor
	; bh = page (0x00)
	int 0x10
	ret

unsupported_bios:
	mov si, .message
	call print_string
	; fall through
exit:
	cli
	hlt
	jmp exit

unsupported_bios.message: db "Unsupported BIOS", 0
stage1.message: db "Leaving stage one.", 0

LBA_Packet:
	db 16 ; constant size
	db 0  ; unspecified
LBA_Packet.sectors: dw 1
LBA_Packet.buffer:  dd 0x0800
LBA_Packet.start:   dd 1
	dd 0 ; used for 48bit LBA indexing

partition: dd 0
drive_number: db 0
times (510 - 64) - ($ - $$) db 0
partition_table: times 64 db 0

; Magic number used by BIOS to recognize a valid bootsector
dw 0xAA55

stage2:
	mov si, .message.entry
	call print_stringln

	mov si, .message.hello
	call print_stringln

	; save partition LBA offset
	mov eax, [partition_table + 8]
	mov [partition], eax

	; prepare LBA read request
	mov word   [LBA_Packet.sectors], 1
	mov dword  [LBA_Packet.buffer], 0x7c00
	mov dword  [LBA_Packet.start], eax

	; read volume bootloader
	mov ah, 0x42 ; read LBA
	mov si, LBA_Packet
	mov dl, [drive_number]
	int 0x13 ; BIOS mass storage
	jc exit ; TODO error handling

	; jmp to loaded volume bootloader code
	mov ax, 0x7c00
	jmp ax

stage2.message.entry: db "Now in stage two.", 0
stage2.message.hello: db "Hello, world!", 0
