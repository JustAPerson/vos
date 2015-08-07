.PHONY: run build clean
run: build
	qemu-system-i386 bin/vos.bin

clean:
	rm -rf bin/

build: bin bin/vos.bin

bin/vos.bin: bin/boot/alpha.bin bin/boot/beta.bin bin/util/mkdisk bin/fs/
	./bin/util/mkdisk -b bin/boot/alpha.bin -v bin/boot/beta.bin \
		              -o bin/vos.bin bin/fs/

bin/boot/%.bin: src/boot/%.s
	nasm -f bin $< -o $@

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

