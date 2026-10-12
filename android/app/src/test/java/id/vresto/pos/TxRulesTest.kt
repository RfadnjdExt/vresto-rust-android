package id.vresto.pos

import id.vresto.pos.core.TxRules
import id.vresto.pos.core.formatHour
import id.vresto.pos.core.formatLocal
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.ZoneId

class TxRulesTest {

    private fun void(status: String, order: String?, refunded: Long, trxShift: Long?, trxUser: Long, myShift: Long?, myUser: Long, admin: Boolean) =
        TxRules.canVoid(status, order, refunded, trxShift, trxUser, myShift, myUser, admin)

    @Test fun voidAllowedForOwnOpenShiftSale() {
        assertTrue(void("confirmed", "preparing", 0, 1, 2, 1, 2, false))
        assertTrue(void("confirmed", "ready", 0, 1, 2, 1, 2, false))
    }

    @Test fun voidRejectedWhenOrderFinishedOrAlreadyRefunded() {
        assertFalse("pesanan selesai → refund", void("confirmed", "completed", 0, 1, 2, 1, 2, false))
        assertFalse(void("confirmed", "cancelled", 0, 1, 2, 1, 2, false))
        assertFalse("sudah pernah direfund", void("partial_refund", "preparing", 1000, 1, 2, 1, 2, false))
    }

    @Test fun cashierCannotVoidOthersShiftButAdminMayTry() {
        assertFalse(void("confirmed", "preparing", 0, 5, 9, 1, 2, false))
        assertFalse("kasir tanpa shift", void("confirmed", "preparing", 0, 1, 2, null, 2, false))
        assertFalse("transaksi tanpa shift", void("confirmed", "preparing", 0, null, 2, 1, 2, false))
        assertTrue("admin: server yang memeriksa shift terbuka", void("confirmed", "preparing", 0, 5, 9, null, 1, true))
    }

    @Test fun debtSaleVoidByCreatorOrAdminOnly() {
        assertTrue(void("unpaid", "preparing", 0, 1, 2, null, 2, false))
        assertFalse(void("unpaid", "preparing", 0, 1, 9, null, 2, false))
        assertTrue(void("unpaid", "preparing", 0, 1, 9, null, 1, true))
    }

    @Test fun terminalStatusesCannotBeVoided() {
        listOf("voided", "refunded", "pending", "failed", "cancelled").forEach { assertFalse(it, void(it, "preparing", 0, 1, 2, 1, 2, true)) }
    }

    @Test fun refundPolicy() {
        assertTrue(TxRules.canRefund("confirmed", 65_550, isAdmin = true, refundRequiresAdmin = true))
        assertFalse("kebijakan default: hanya admin", TxRules.canRefund("confirmed", 65_550, isAdmin = false, refundRequiresAdmin = true))
        assertTrue("kebijakan dilonggarkan", TxRules.canRefund("confirmed", 65_550, isAdmin = false, refundRequiresAdmin = false))
        assertTrue("refund lanjutan", TxRules.canRefund("partial_refund", 55_550, true, true))
        assertFalse("penuh", TxRules.canRefund("refunded", 0, true, true))
        assertFalse("sisa 0", TxRules.canRefund("partial_refund", 0, true, true))
        assertFalse("hutang tak bisa refund", TxRules.canRefund("unpaid", 34_500, true, true))
        assertFalse("void tak bisa refund", TxRules.canRefund("voided", 11_500, true, true))
    }

    @Test fun refundAmountAndRestock() {
        assertEquals(55_550L, TxRules.remainingRefundable(65_550, 10_000))
        assertEquals(0L, TxRules.remainingRefundable(100, 500))
        assertNotNull(TxRules.validateRefundAmount(0, 100))
        assertTrue(TxRules.validateRefundAmount(70_000, 65_550)!!.contains("Rp65.550"))
        assertNull(TxRules.validateRefundAmount(65_550, 65_550))
        assertTrue(TxRules.canRestock(55_550, 55_550, "completed"))
        assertFalse("sebagian tidak boleh restock", TxRules.canRestock(10_000, 55_550, "completed"))
        assertFalse("pesanan belum selesai (stok belum berkurang)", TxRules.canRestock(55_550, 55_550, "preparing"))
        assertTrue(TxRules.mustRefundFully("points")); assertFalse(TxRules.mustRefundFully("cash"))
    }

    @Test fun debtRepayRules() {
        assertTrue(TxRules.canRepay("OPEN", 100)); assertTrue(TxRules.canRepay("PARTIAL", 1))
        assertFalse(TxRules.canRepay("PAID", 0)); assertFalse(TxRules.canRepay("VOID", 0)); assertFalse(TxRules.canRepay("OPEN", 0))
        assertTrue(TxRules.validateRepayAmount(30_000, 24_500)!!.contains("Rp24.500"))
        assertNull(TxRules.validateRepayAmount(24_500, 24_500))
        assertEquals(listOf("cash", "qris", "transfer"), TxRules.repayMethods(listOf("cash", "qris", "debt", "points", "transfer")))
    }

    @Test fun reversePaymentPermission() {

        assertTrue(TxRules.canReversePayment(false, false, 2, 1, 2, 1))
        assertFalse("shift penerimaan sudah lain", TxRules.canReversePayment(false, false, 2, 1, 2, 2))
        assertFalse("bukan penerima", TxRules.canReversePayment(false, false, 9, 1, 2, 1))
        assertFalse("tanpa shift", TxRules.canReversePayment(false, false, 2, 1, 2, null))
        assertTrue("admin lintas shift", TxRules.canReversePayment(false, true, 9, 1, 1, null))
        assertFalse("sudah dibatalkan", TxRules.canReversePayment(true, true, 2, 1, 2, 1))
    }

    @Test fun labels() {
        assertEquals("Refund sebagian", TxRules.statusLabel("partial_refund"))
        assertEquals("Dibatalkan", TxRules.debtStatusLabel("VOID"))
        assertEquals("aneh", TxRules.statusLabel("aneh"))
    }

    @Test fun timeFormatting() {
        val wib = ZoneId.of("Asia/Jakarta")
        assertEquals("11/10/2026 09:51", formatLocal("2026-10-11T02:51:06Z", wib))
        assertEquals("12/10/2026 03:00", formatLocal("2026-10-11T20:00:00Z", wib))
        assertEquals("bukan-tanggal", formatLocal("bukan-tanggal", wib))
        assertEquals("", formatLocal(null, wib))
        assertEquals("09:51", formatHour("2026-10-11T02:51:06Z", wib))
    }
}
