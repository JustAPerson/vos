BITS 16
ORG 0x7C00

; main:
; 	mov al, 'A'
; 	mov ah, 0x0E
; 	mov bh, 0x00
; 	mov bl, 0x07

; 	int 0x10

main:
	xor ax, ax
	mov ds, ax
	mov ss, ax
	mov sp, 0x7C00
	mov si, message
	call print_string

	jmp exit

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

exit:
	; nothing to return to, just hang
	cli
	hlt
	jmp exit

message db "Hello, world!", 0

times 510 - ($ - $$) db 0

; Magic number used by BIOS to recognize a valid bootsector
dw 0xAA55

