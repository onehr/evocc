# Minimal Kbuild-shaped producer graph. `prepare` intentionally does not
# generate a directory-local table; final consumers require vmlinux_o first.
.PHONY: prepare vmlinux_o FORCE
prepare:

vmlinux_o: consumer.o

prepared-offsets.h: FORCE
	@printf '#define PREPARED_OFFSET %s\n' '$(VALUE)' > $@.tmp
	@cmp -s $@.tmp $@ && rm $@.tmp || mv $@.tmp $@

consumer.o: /work/src/kernel-prepared-consumer.c prepared-offsets.h
	gcc-14 -I. -DRESUME_NAME=consumer -c $< -o $@
