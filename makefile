.PHONY: run build clean
run: build
	qemu-system-i386 bin/vos.bin

clean:
	rm -rf bin/

build: bin bin/vos.bin

bin/vos.bin: bin/boot/alpha.bin bin/boot/beta.bin bin/util/mkdisk bin/fs/
	./bin/util/mkdisk -b bin/boot/alpha.bin -v bin/boot/beta.bin \
		              -o bin/vos.bin bin/fs/

bin/boot/alpha.bin: src/boot/alpha.s
	nasm -f bin $< -o $@

bin/boot/beta.bin: bin/boot/beta.o bin/boot/gamma.o
	ld -T src/boot/gamma.ld -Lbin/boot/ -o $@

bin/boot/beta.o: src/boot/beta.s
	nasm -f elf32 -o $@ $<
bin/boot/gamma.o: src/boot/gamma.rs
	rustc --target etc/targets/real.json -C opt-level=2 -o $@ $<

ROPT = -C opt-level=2
RFLAGS = $(RTARGET) $(ROPT) $(REXTERN)

# TODO: figure out better way to rebuild utilities
.FORCE:
bin/util/%: .FORCE
	cd src/util/$(*F)/ && cargo rustc -- $(RFLAGS)
	cp src/util/$(*F)/target/debug/$(*F) $@

bin:
	mkdir -p bin
	mkdir bin/boot/ bin/util/ bin/fs/

bin/fs/: ;

