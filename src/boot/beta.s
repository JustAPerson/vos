BITS 16 ; execution begins in real mode

; bootsector is loaded at 0x0000:0x7c00
ORG 0x7C00

; File system header contents go hear
; `mkdisk` will provide the proper values
times 90 db 0

stage1:
	mov si, .message
	call print_stringln

	jmp exit


print_string:
	mov ah, 0x0E
	mov bh, 0x00
	mov bl, 0x07
.loop:
	mov al, [si]
	inc si
	int 0x10
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

exit:
	cli
	hlt
	jmp exit

stage1.message: db "Hiya, https://livecoding.tv/", 0

times (510) - ($ - $$) db 0

; Magic number used by BIOS to recognize a valid bootsector
dw 0xAA55
