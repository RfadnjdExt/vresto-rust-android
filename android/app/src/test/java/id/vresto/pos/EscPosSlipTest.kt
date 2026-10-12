package id.vresto.pos

import id.vresto.pos.print.DebtSlip
import id.vresto.pos.print.EscPos
import id.vresto.pos.print.RefundSlip
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class EscPosSlipTest {
    private val refund = RefundSlip("Warung Demo", "Jl. Contoh 1", "0812", "TRX-20261011-0002", "DI-20261011-0002", "11/10/2026 09:51", "Administrator",
        "Menu kurang & pelanggan komplain, mohon maaf", 10_000, "Tunai", cashOut = true, totalRefunded = 10_000, remaining = 55_550, isFull = false)
    private val debt = DebtSlip("Warung Demo", "", "", "Budi Santoso", listOf("DI-20261011-0001", "TA-20261011-0002"), "11/10/2026 09:51", "Kasir",
        40_000, "QRIS", 14_000, "transfer Budi")

    private fun text(b: ByteArray) = String(b, Charsets.ISO_8859_1)

    private fun printable(b: ByteArray) = text(b).replace(Regex("\u001B[@]|\u001B[aE.d]|\u001D[V!].?.?|\u001B."), "")

    @Test fun refundSlipStructureAndContent() {
        val b = EscPos.renderRefund(refund)
        assertEquals(0x1B.toByte(), b[0]); assertEquals(0x40.toByte(), b[1])
        assertEquals(0x1D.toByte(), b[b.size - 4]); assertEquals(0x56.toByte(), b[b.size - 3])
        val t = text(b)
        assertTrue(t.contains("Rp10.000") && t.contains("REFUND SEBAGIAN") && t.contains("kas keluar") && t.contains("Rp55.550"))
        val full = text(EscPos.renderRefund(refund.copy(isFull = true, remaining = 0, cashOut = false)))
        assertTrue(full.contains("REFUND PENUH")); assertFalse(full.contains("kas keluar"))
    }

    @Test fun debtSlipContent() {
        val t = text(EscPos.renderDebtPayment(debt))
        assertTrue(t.contains("Budi Santoso") && t.contains("Rp40.000") && t.contains("Rp14.000") && t.contains("BELUM LUNAS") && t.contains("DI-20261011-0001"))
        val paid = text(EscPos.renderDebtPayment(debt.copy(remainingAfter = 0)))
        assertTrue(paid.contains("LUNAS")); assertFalse(paid.contains("BELUM"))
    }

    @Test fun allLinesFitPaperWidth() {
        listOf(EscPos.renderRefund(refund), EscPos.renderDebtPayment(debt)).forEach { b ->
            val tooLong = printable(b).split("\n").filter { it.length > EscPos.COLS + 2 }
            assertTrue("baris melebihi 32 kolom: $tooLong", tooLong.isEmpty())
        }
    }
}
