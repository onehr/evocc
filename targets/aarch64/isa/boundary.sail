// Project domain boundary, not vendor instruction semantics.
// These no-memory leaf fixtures never access SP. Refuse such an instruction
// rather than supplying a private implementation of stack-pointer semantics.
val SP_read : unit -> bits(64)
function SP_read() = throw(Error_Undefined())

val SP_set : bits(64) -> unit
function SP_set(value) = throw(Error_Undefined())
