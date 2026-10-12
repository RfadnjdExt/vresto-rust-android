package id.vresto.pos

import id.vresto.pos.print.EscPos
import id.vresto.pos.print.Receipt
import id.vresto.pos.print.ReceiptLine
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class EscPosTest {
    private val receipt = Receipt(
        shopName = "Warung Demo", orderNumber = "DI-1", orderTypeLabel = "Dine In", dateTime = "11/10/2026 09:15",
        lines = listOf(ReceiptLine("Frisian Flag Dingin Ekstra Panjang Sekali Namanya Biar Dibungkus", 1, 14_000, listOf("+ Telur x2"))),
        subtotal = 14_000, total = 14_000, paymentLabel = "Tunai", paid = 20_000, change = 6_000, queueNumber = 7,
    )

    @Test fun startsWithInitAndEndsWithCut() {
        val b = EscPos.render(receipt)
        assertEquals(0x1B.toByte(), b[0]); assertEquals(0x40.toByte(), b[1])
        assertEquals(0x1D.toByte(), b[b.size - 4]); assertEquals(0x56.toByte(), b[b.size - 3])
    }

    @Test fun textLinesFitPaperWidth() {
        assertEquals(32, EscPos.twoCol("TOTAL", "Rp82.800").length)
        assertTrue(EscPos.wrap(receipt.lines[0].name, EscPos.COLS).all { it.length <= EscPos.COLS })
        assertEquals("Caf?", EscPos.ascii("Café"))
    }
}
