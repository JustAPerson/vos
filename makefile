.PHONY: run
run: build
	qemu-system-i386 bin/vos.bin

.PHONY: build
build: bin bin/vos.bin
bin/vos.bin: bin/alpha.o
	ld -L bin/ -o $@ -T src/boot/boot.ld

TARGET16 := --target etc/targets/real.json

bin/alpha.o: RTARGET := $(TARGET16)
bin/alpha.o: src/boot/alpha.rs

%.o:
	rustc --emit obj,dep-info --out-dir bin/ $(RTARGET) $<

# make tries to rebuild .d files through the above %.o rule
# (e.g. tries resolving a rule for file.d.o)
# prevent this strange behavior using this rule
%.d: ;

bin:
	mkdir -p bin

.PHONY: clean
clean:
	rm -rf bin/

-include $(wildcard bin/*.d)
