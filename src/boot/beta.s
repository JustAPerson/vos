BITS 16 ; execution begins in real mode

; bootsector is loaded at 0x0000:0x7c00
section beta vstart=0x7c00

; File system header contents go hear
; `mkdisk` will provide the proper values
times 90 db 0

stage1:
	mov si, .message
	call print_stringln

	mov eax, [0x0600 + 446 + 8]
	add eax, 1
	mov [LBA_Packet.start], eax

	mov ah, 0x42 ; read LBA
	mov si, LBA_Packet
	mov dl, 0x80 ; TODO drive number
	int 0x13 ; BIOS mass storage
	jc exit ; TODO error handling

	jmp stage2


print_stringln:
	mov ah, 0x0E
	mov bh, 0x00
	mov bl, 0x07
.loop:
	mov al, [si]
	inc si
	int 0x10
	or al, al
	jnz .loop

	mov al, 0x0a
	int 0x10
	mov al, 0x0d
	int 0x10
	ret

exit:
	cli
.loop
	hlt
	jmp .loop

stage1.message: db "volume bootloader stage1", 0

LBA_Packet:
	db 16 ; constant size
	db 0  ; unspecified
LBA_Packet.sectors: dw 4
LBA_Packet.buffer:  dd 0x7e00
LBA_Packet.start:   dd 1
	dd 0 ; used for 48bit LBA indexing

times (510) - ($ - $$) db 0

; Magic number used by BIOS to recognize a valid bootsector
dw 0xAA55

stage2:
	mov si, .message
	call print_stringln

	jmp exit

stage2.message: db "volume bootloader stage2", 0
