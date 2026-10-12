package id.vresto.pos.print

import id.vresto.pos.core.rupiah
import java.io.ByteArrayOutputStream

data class ReceiptLine(val name: String, val qty: Int, val total: Long, val extras: List<String> = emptyList())

data class Receipt(
    val shopName: String,
    val address: String = "",
    val phone: String = "",
    val orderNumber: String,
    val queueNumber: Int? = null,
    val orderTypeLabel: String,
    val tableName: String? = null,
    val cashier: String? = null,
    val dateTime: String,
    val lines: List<ReceiptLine>,
    val subtotal: Long,
    val discount: Long = 0,
    val tax: Long = 0,
    val service: Long = 0,
    val packaging: Long = 0,
    val shipping: Long = 0,
    val total: Long,
    val paymentLabel: String,
    val paid: Long,
    val change: Long = 0,
)

data class RefundSlip(
    val shopName: String, val address: String = "", val phone: String = "",
    val trxNumber: String, val orderNumber: String?, val dateTime: String, val cashier: String?,
    val reason: String, val amount: Long, val methodLabel: String, val cashOut: Boolean,
    val totalRefunded: Long, val remaining: Long, val isFull: Boolean,
)

data class DebtSlip(
    val shopName: String, val address: String = "", val phone: String = "",
    val customerName: String, val orderNumbers: List<String>, val dateTime: String, val cashier: String?,
    val amount: Long, val methodLabel: String, val remainingAfter: Long, val notes: String? = null,
)

object EscPos {
    const val COLS = 32

    private val INIT = byteArrayOf(0x1B, 0x40)
    private val LEFT = byteArrayOf(0x1B, 0x61, 0x00)
    private val CENTER = byteArrayOf(0x1B, 0x61, 0x01)
    private val BOLD_ON = byteArrayOf(0x1B, 0x45, 0x01)
    private val BOLD_OFF = byteArrayOf(0x1B, 0x45, 0x00)
    private val BIG = byteArrayOf(0x1D, 0x21, 0x11)
    private val NORMAL = byteArrayOf(0x1D, 0x21, 0x00)
    private val CUT = byteArrayOf(0x1D, 0x56, 0x42, 0x00)

    fun ascii(s: String): String = s.map { if (it.code in 32..126) it else '?' }.joinToString("")

    fun wrap(text: String, width: Int): List<String> {
        val out = mutableListOf<String>()
        var line = StringBuilder()
        for (word in ascii(text).split(' ').filter { it.isNotEmpty() }) {
            var w = word
            while (w.length > width) {
                if (line.isNotEmpty()) { out += line.toString(); line = StringBuilder() }
                out += w.take(width); w = w.drop(width)
            }
            if (line.isEmpty()) line.append(w)
            else if (line.length + 1 + w.length <= width) line.append(' ').append(w)
            else { out += line.toString(); line = StringBuilder(w) }
        }
        if (line.isNotEmpty()) out += line.toString()
        return out.ifEmpty { listOf("") }
    }

    fun twoCol(left: String, right: String, width: Int = COLS): String {
        val l = ascii(left); val r = ascii(right)
        val room = width - r.length - 1
        return (if (l.length > room) l.take(maxOf(0, room)) else l).padEnd(width - r.length) + r
    }

    fun render(r: Receipt, cols: Int = COLS): ByteArray {
        val out = ByteArrayOutputStream()
        fun raw(b: ByteArray) = out.write(b)
        fun line(s: String = "") { out.write((ascii(s) + "\n").toByteArray(Charsets.US_ASCII)) }
        val rule = "-".repeat(cols)

        raw(INIT); raw(CENTER); raw(BOLD_ON); raw(BIG)
        wrap(r.shopName, cols / 2).forEach(::line)
        raw(NORMAL); raw(BOLD_OFF)
        if (r.address.isNotBlank()) wrap(r.address, cols).forEach(::line)
        if (r.phone.isNotBlank()) line(r.phone)
        raw(LEFT); line(rule)

        line(twoCol("No", r.orderNumber, cols))
        line(twoCol("Tanggal", r.dateTime, cols))
        line(twoCol("Tipe", r.orderTypeLabel + (r.tableName?.let { " / $it" } ?: ""), cols))
        r.cashier?.let { line(twoCol("Kasir", it, cols)) }
        line(rule)

        for (l in r.lines) {
            wrap("${l.qty}x ${l.name}", cols).forEach(::line)
            l.extras.forEach { e -> wrap(e, cols - 2).forEach { line("  $it") } }
            line(twoCol("", rupiah(l.total), cols))
        }
        line(rule)

        line(twoCol("Subtotal", rupiah(r.subtotal), cols))
        if (r.discount > 0) line(twoCol("Diskon", "-" + rupiah(r.discount), cols))
        if (r.tax > 0) line(twoCol("Pajak", rupiah(r.tax), cols))
        if (r.service > 0) line(twoCol("Service", rupiah(r.service), cols))
        if (r.packaging > 0) line(twoCol("Kemasan", rupiah(r.packaging), cols))
        if (r.shipping > 0) line(twoCol("Ongkir", rupiah(r.shipping), cols))
        raw(BOLD_ON); line(twoCol("TOTAL", rupiah(r.total), cols)); raw(BOLD_OFF)
        line(twoCol("Bayar (${r.paymentLabel})", rupiah(r.paid), cols))
        if (r.change > 0) line(twoCol("Kembali", rupiah(r.change), cols))

        if (r.queueNumber != null) {
            line(rule); raw(CENTER); line("No. Antrean"); raw(BIG); raw(BOLD_ON)
            line(r.queueNumber.toString().padStart(3, '0')); raw(NORMAL); raw(BOLD_OFF)
        }
        raw(CENTER); line(); line("Terima kasih!"); line(); line(); line()
        raw(CUT)
        return out.toByteArray()
    }

    private class Doc(val cols: Int) {
        val out = ByteArrayOutputStream()
        fun raw(b: ByteArray) = out.write(b)
        fun line(s: String = "") { out.write((ascii(s) + "\n").toByteArray(Charsets.US_ASCII)) }
        fun kv(k: String, v: String) = line(twoCol(k, v, cols))
        fun rule() = line("-".repeat(cols))
    }

    private fun header(d: Doc, shop: String, address: String, phone: String, title: String) {
        d.raw(INIT); d.raw(CENTER); d.raw(BOLD_ON); d.raw(BIG)
        wrap(shop, d.cols / 2).forEach(d::line)
        d.raw(NORMAL)
        d.line(title)
        d.raw(BOLD_OFF)
        if (address.isNotBlank()) wrap(address, d.cols).forEach(d::line)
        if (phone.isNotBlank()) d.line(phone)
        d.raw(LEFT); d.rule()
    }

    private fun footer(d: Doc) {
        d.raw(CENTER); d.line(); d.line("Simpan bukti ini"); d.line(); d.line(); d.line(); d.raw(CUT)
    }

    fun renderRefund(r: RefundSlip, cols: Int = COLS): ByteArray {
        val d = Doc(cols)
        header(d, r.shopName, r.address, r.phone, "BUKTI REFUND")
        d.kv("Transaksi", r.trxNumber)
        r.orderNumber?.let { d.kv("Pesanan", it) }
        d.kv("Tanggal", r.dateTime)
        r.cashier?.let { d.kv("Diproses", it) }
        d.rule()
        d.raw(BOLD_ON); d.kv("REFUND", rupiah(r.amount)); d.raw(BOLD_OFF)
        d.kv("Metode", r.methodLabel + if (r.cashOut) " (kas keluar)" else "")
        d.rule()
        d.line("Alasan:")
        wrap(r.reason, cols - 2).forEach { d.line("  $it") }
        d.rule()
        d.kv("Total direfund", rupiah(r.totalRefunded))
        d.kv("Sisa bisa direfund", rupiah(r.remaining))
        d.raw(CENTER); d.raw(BOLD_ON); d.line(if (r.isFull) "REFUND PENUH" else "REFUND SEBAGIAN"); d.raw(BOLD_OFF)
        footer(d)
        return d.out.toByteArray()
    }

    fun renderDebtPayment(s: DebtSlip, cols: Int = COLS): ByteArray {
        val d = Doc(cols)
        header(d, s.shopName, s.address, s.phone, "BUKTI PELUNASAN HUTANG")
        d.kv("Pelanggan", s.customerName)
        if (s.orderNumbers.isNotEmpty()) {
            d.line("Pesanan:")
            s.orderNumbers.forEach { d.line("  $it") }
        }
        d.kv("Tanggal", s.dateTime)
        s.cashier?.let { d.kv("Diterima", it) }
        d.rule()
        d.raw(BOLD_ON); d.kv("DIBAYAR", rupiah(s.amount)); d.raw(BOLD_OFF)
        d.kv("Metode", s.methodLabel)
        s.notes?.takeIf { it.isNotBlank() }?.let { wrap("Catatan: $it", cols).forEach(d::line) }
        d.rule()
        d.raw(BOLD_ON); d.kv("SISA HUTANG", rupiah(s.remainingAfter)); d.raw(BOLD_OFF)
        d.raw(CENTER); d.line(if (s.remainingAfter == 0L) "LUNAS" else "BELUM LUNAS")
        footer(d)
        return d.out.toByteArray()
    }
}
