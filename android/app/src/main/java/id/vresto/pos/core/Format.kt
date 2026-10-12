package id.vresto.pos.core

fun rupiah(n: Long): String {
    val neg = n < 0
    val digits = kotlin.math.abs(n).toString()
    val grouped = digits.reversed().chunked(3).joinToString(".").reversed()
    return (if (neg) "-Rp" else "Rp") + grouped
}
