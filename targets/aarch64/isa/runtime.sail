// Project reset and observation glue; arithmetic and decoding are upstream.
val evocc_model_reset : (bits(64), bits(64)) -> unit
function evocc_model_reset(a, b) = {
    foreach (i from 0 to 30) { write_gpr(i, 0x0000000000000000) };
    R0 = a;
    R1 = b;
    PSTATE = undefined;
    PSTATE.N = 0b0;
    PSTATE.Z = 0b0;
    PSTATE.C = 0b0;
    PSTATE.V = 0b0;
    SEE = 0
}

val evocc_model_step : bits(32) -> unit
function evocc_model_step(opcode) = {
    SEE = 0;
    __DecodeA64(0, opcode)
}

val evocc_model_result : unit -> bits(64)
function evocc_model_result() = R0

val evocc_model_flags : unit -> bits(4)
function evocc_model_flags() = PSTATE.N @ PSTATE.Z @ PSTATE.C @ PSTATE.V

val main : unit -> unit
function main() = ()
