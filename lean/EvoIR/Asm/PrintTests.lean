import EvoIR.Asm.Print
import EvoIR.Asm.Reader

/-!
Focused tests of the assembly printer and reader. The expected strings are
copied from the actual output of `evo-ir-asm` for musl `abs`.
-/
namespace EvoIR.Asm.PrintTests

def absInput : String :=
  "[\"core-asm-v1\",\"aarch64-unknown-linux-gnu\",\"checked\",\"EvoCC 0.0.0 aarch64-unknown-linux-gnu\"," ++
  "[[\"abs\",true,[[\"stp_pre\",29,30,-16],[\"mov_from_sp\",29],[\"label\",0],[\"cmp_imm\",\"w\",0,0]," ++
  "[\"b_cond\",\"le\",2],[\"label\",1],[\"mov\",\"w\",1,0],[\"b\",3],[\"label\",2],[\"movz\",\"w\",2,0]," ++
  "[\"flag_alu\",\"subs\",\"w\",0,2,0],[\"b_cond\",\"vs\",4],[\"label\",5],[\"mov\",\"w\",1,0],[\"label\",3]," ++
  "[\"mov\",\"w\",0,1],[\"ldp_post\",29,30,16],[\"ret\"],[\"label\",4],[\"brk\",3004]],[0,[],[],[],0]]]]"

def absText : String :=
  "// evocc-native-ub-checks=checked\n.text\n\n.globl abs\n.p2align 2\n.type abs, %function\nabs:\n" ++
  "    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n.Labs_block0:\n    cmp w0, #0\n" ++
  "    b.le .Labs_block2\n.Labs_block1:\n    mov w1, w0\n    b .Labs_block3\n.Labs_block2:\n" ++
  "    movz w2, #0\n    subs w0, w2, w0\n    b.vs .Labs_block4\n.Labs_block5:\n    mov w1, w0\n" ++
  ".Labs_block3:\n    mov w0, w1\n    ldp x29, x30, [sp], #16\n    ret\n.Labs_block4:\n" ++
  "    brk #3004\n.size abs, .-abs\n\n.ident \"EvoCC 0.0.0 aarch64-unknown-linux-gnu\"\n"

def absFile : Option File := (Reader.readFile absInput).toOption.map Prod.fst

#guard absFile.isSome
#guard absFile.map (printMatches · absText) == some true

-- An exchanged register pair in one line is a mismatch.
#guard absFile.map (printMatches · (absText.replace "subs w0, w2, w0" "subs w0, w0, w2")) == some false

-- A missing line is a mismatch.
#guard absFile.map (printMatches · (absText.replace "    mov x29, sp\n" "")) == some false

-- A different profile comment is a mismatch.
#guard absFile.map (printMatches · (absText.replace "checked" "unchecked")) == some false

-- A changed condition is a mismatch, and the first difference is line 12.
#guard absFile.map (firstDifference · (absText.replace "b.le" "b.lt")) == some (some 12)

-- Each line form prints as the emitter writes it.
#guard (Inst.ldr .w (.r 0) (.r 1) 0).print "f" == "ldr w0, [x1, #0]"
#guard (Inst.str .x (.r 28) .sp 248).print "f" == "str x28, [sp, #248]"
#guard (Inst.ldr .x (.r 8) (.r 29) 16).print "f" == "ldr x8, [x29, #16]"
#guard (Inst.addFromSp (.r 19) 0).print "f" == "add x19, sp, #0"
#guard (Inst.subSp 336).print "f" == "sub sp, sp, #336"
#guard (Inst.addSp 336).print "f" == "add sp, sp, #336"
#guard (Inst.movk .x (.r 3) 7 32).print "f" == "movk x3, #7, lsl #32"
#guard (Inst.wideMul .smull (.r 0) (.r 1) (.r 2)).print "f" == "smull x0, w1, w2"
#guard (Inst.wideMul .smulh (.r 3) (.r 0) (.r 1)).print "f" == "smulh x3, x0, x1"
#guard (Inst.extend .sxtw .x (.r 0) (.r 0)).print "f" == "sxtw x0, w0"
#guard (Inst.tb false .w (.r 3) 0 5).print "f" == "tbz w3, #0, .Lf_block5"
#guard (Inst.cb true .x (.r 16) 2).print "f" == "cbnz x16, .Lf_block2"
#guard (Inst.cset .w (.r 3) .ne).print "f" == "cset w3, ne"
#guard (Inst.alu .add .w .zr (.r 1) (.r 2)).print "f" == "add wzr, w1, w2"

-- The reader refuses register 31, an unknown tag and a bad symbol name.
#guard (Reader.instruction (Lean.Json.arr #["mov_from_sp", 31])).isNone
#guard (Reader.instruction (Lean.Json.arr #["adr", 1, 2])).isNone
#guard !Reader.symbolName "a b"
#guard !Reader.symbolName "1a"
#guard Reader.symbolName "loop_join"

end EvoIR.Asm.PrintTests
