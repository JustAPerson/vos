.PHONY: run build clean clean_cargo fclean
run: build
	qemu-system-i386 bin/vos.bin

fclean: clean_cargo clean
clean:
	rm -rf bin/

clean_cargo:
	cd src/lib/bootalloc/ && cargo clean
	cd src/lib/disk/ && cargo clean
	cd src/lib/libc/ && cargo clean
	cd src/lib/std/ && cargo clean

build: bin bin/vos.bin

bin/vos.bin: bin/boot/alpha.bin bin/boot/beta.bin bin/util/mkdisk bin/fs/
	./bin/util/mkdisk -b bin/boot/alpha.bin -v bin/boot/beta.bin \
		              -o bin/vos.bin bin/fs/


bin/boot/alpha.bin: src/boot/alpha.s
	nasm -f bin $< -o $@
bin/boot/beta.o: src/boot/beta.s
	nasm -f elf32 -o $@ $<

bin/boot/gamma.o: private RTARGET = --target etc/targets/real.json
bin/boot/gamma.o: private REXTERN = $(ESTD16) --extern disk=bin/lib16/libdisk.rlib
bin/boot/gamma.o: src/boot/gamma.rs bin/lib16/libdisk.rlib
	rustc -o $@ $< $(RFLAGS) --emit obj

bin/boot/lib%.o:
	rustc -o $@ $< $(RFLAGS)

bin/boot/beta.bin: bin/boot/beta.o bin/boot/gamma.o bin/lib16/libstd.o
bin/boot/beta.bin: bin/rust16/liballoc.o bin/rust16/libcore.o bin/rust16/librustc_unicode.o
bin/boot/beta.bin: bin/rust16/libcollections.o bin/lib16/libbootalloc.o bin/lib16/libdisk.o
bin/boot/beta.bin: bin/lib16/librlibc.o
	ld -T src/boot/gamma.ld -Lbin/boot/ -Lbin/rust16/ -Lbin/lib16/ -o $@

# RTARGET, RCFG, RFEATURES, RCFLAGS
ROPT = -C opt-level=2
RFLAGS = $(RTARGET) $(ROPT) $(REXTERN) $(RCFG)

# TODO: figure out better way to rebuild utilities
.FORCE:
bin/util/%: .FORCE
	cd src/util/$(*F)/ && cargo rustc $(RCFLAGS) -- $(RFLAGS)
	cp src/util/$(*F)/target/debug/$(*F) $@

LIBCORE16 := bin/rust16/libcore.rlib
LIBALLOC16 := bin/rust16/liballoc.rlib
LIBCOLLECTIONS16 := bin/rust16/libcollections.rlib
LIBRUSTC_UNICODE16 := bin/rust16/librustc_unicode.rlib

LIBBOOTALLOC16 := bin/lib16/libbootalloc.rlib
LIBSTD16 := bin/lib16/libstd.rlib

ECORE16 = --extern core=$(LIBCORE16)
# EALLOC16 := --extern bootalloc=$(LIBBOOTALLOC16) --extern alloc=$(LIBALLOC16)
# ECOLLECTIONS16 := --extern collections=$(LIBCOLLECTIONS16) --extern rustc_unicode=$(LIBRUSTC_UNICODE16)
EALLOC16 = --extern alloc=$(LIBALLOC16)
EUNICODE16 = --extern rustc_unicode=$(LIBRUSTC_UNICODE16)
ECOLLECTIONS16 = --extern collections=$(LIBCOLLECTIONS16)

# ELIBSTD16 = --extern std=$(LIBSTD16) $(ECORE16) $(EALLOC16) $(ECOLLECTIONS16) $(EUNICODE16)
ESTD16 = --extern std=$(LIBSTD16) $(LPATH16)

LPATH16 := -L dependency=bin/lib16/ -L dependency=bin/rust16/

bin/lib16/libdisk.rlib: bin/lib16/libstd.rlib
bin/lib16/libdisk.rlib: private RFEATURES := "bootdriver"
bin/lib16/libdisk.rlib: REXTERN = $(ESTD16)

bin/lib16/libstd.rlib: bin/rust16/libcore.rlib bin/rust16/liballoc.rlib .FORCE
bin/lib16/libstd.rlib: bin/rust16/libcollections.rlib
bin/lib16/libstd.rlib: REXTERN = $(ECORE16) $(EALLOC16) $(ECOLLECTIONS16) $(EUNICODE16) $(LPATH16)


bin/lib16/libstd.o: bin/rust16/libcore.rlib bin/rust16/liballoc.rlib 
bin/lib16/libstd.o: bin/rust16/libcollections.rlib
bin/lib16/libstd.o: REXTERN := $(ECORE16) $(EALLOC16) $(ECOLLECTIONS16) $(EUNICODE16) $(LPATH16)
bin/lib16/libbootalloc.o: REXTERN := $(ECORE16) $(EALLOC16) $(ECOLLECTIONS16) $(EUNICODE16) $(LPATH16)

bin/lib16/librlibc.o: bin/rust16/libcore.rlib
bin/lib16/librlibc.o: REXTERN = $(ECORE16)

bin/lib16/libdisk.o: bin/lib16/libstd.rlib
bin/lib16/libdisk.o: REXTERN = $(ESTD16) $(LPATH16)

# $(LIBBOOTALLOC16): src/lib/bootalloc/src/lib.rs
# 	rustc --emit obj --crate-type lib -o $@ $<

bin/lib16/lib%.rlib: bin/rust16/libcore.rlib
	@cargo rustc --manifest-path src/lib/$(*F)/Cargo.toml --target etc/targets/real.json --features=$(RFEATURES) -- $(ROPT) $(REXTERN)
	@cp src/lib/$(*F)/target/real/debug/lib$*.rlib $@

bin/lib16/lib%.o:
	cargo rustc --manifest-path src/lib/$(*F)/Cargo.toml --target etc/targets/real.json --features=$(RFEATURES) -- $(ROPT) $(REXTERN) --emit obj
	@cp src/lib/$(*F)/target/real/debug/$*.o $@

bin/rust16/libcore.rlib: src/rust/src/libcore/lib.rs
bin/rust16/liballoc.rlib: src/rust/src/liballoc/lib.rs $(LIBCORE16) $(LIBBOOTALLOC16)
bin/rust16/liballoc.rlib: private RCFG := --cfg feature=\"external_crate\"
bin/rust16/liballoc.rlib: private REXTERN := --extern external=$(LIBBOOTALLOC16)
bin/rust16/librustc_unicode.rlib: src/rust/src/librustc_unicode/lib.rs $(LIBCORE16)
bin/rust16/libcollections.rlib: src/rust/src/libcollections/lib.rs $(LIBCORE16) \
	                            bin/rust16/liballoc.rlib bin/rust16/librustc_unicode.rlib
bin/rust16/libcollections.rlib: private REXTERN := $(EALLOC16) $(ECOLLECTIONS16)


bin/rust16/%.rlib:
	rustc $(RFLAGS) -Lbin/lib16/ -Lbin/rust16/ --target etc/targets/real.json -o $@ $<

bin/rust16/libcore.o: src/rust/src/libcore/lib.rs
bin/rust16/liballoc.o: src/rust/src/liballoc/lib.rs $(LIBCORE16) $(LIBBOOTALLOC16)
bin/rust16/liballoc.o: private RCFG := --cfg feature=\"external_crate\"
bin/rust16/liballoc.o: REXTERN = --extern external=$(LIBBOOTALLOC16) $(ECORE16)
bin/rust16/librustc_unicode.o: src/rust/src/librustc_unicode/lib.rs $(LIBCORE16)
bin/rust16/librustc_unicode.o: REXTERN = $(ECORE16)
bin/rust16/libcollections.o: src/rust/src/libcollections/lib.rs $(LIBCORE16) \
	                            bin/rust16/liballoc.rlib bin/rust16/librustc_unicode.rlib
bin/rust16/libcollections.o: REXTERN := $(ECORE16) $(EALLOC16) $(EUNICODE16)

bin/rust16/%.o:
	rustc $(RFLAGS) --target etc/targets/real.json -o $@ $< --emit obj $(LPATH16)

src/rust/src/%.rs:
	git submodule update --init

bin:
	mkdir -p bin
	mkdir bin/boot/ bin/util/ bin/fs/
	mkdir bin/lib16/ bin/rust16/

bin/fs/: ;

