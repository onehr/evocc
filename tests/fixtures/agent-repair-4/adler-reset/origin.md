# Source

Adapted from zlib 1.3.2: adler32.c, adler32_z.
The block view, input generator and test driver are corpus scaffolding.
The checksum or encoding loop retains the cited public algorithm.
One injected E3001 defect changes the line in fix-spans.json.
The main return is the processed byte count, not the checksum.
The hidden test reads the checksum for twelve seeds.
