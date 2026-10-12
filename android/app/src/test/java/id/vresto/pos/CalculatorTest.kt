package id.vresto.pos

import id.vresto.pos.core.*
import kotlinx.serialization.json.*
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CalculatorTest {
    private fun JsonObject.long(k: String) = getValue(k).jsonPrimitive.long
    private fun JsonObject.bool(k: String) = getValue(k).jsonPrimitive.boolean

    @Test
    fun matchesSharedVectors() {
        val text = javaClass.getResourceAsStream("/calc_vectors.json")!!.bufferedReader().readText()
        val cases = Json.parseToJsonElement(text).jsonArray.map { it.jsonObject }
        assertTrue(cases.size > 400)
        var ties = 0
        val bad = mutableListOf<String>()
        for (c in cases) {
            val b = c.getValue("billing").jsonObject
            val billing = Billing(b.bool("apply_tax"), b.long("tax_bps"), b.bool("apply_service_dine_in"), b.long("service_bps"), b.bool("apply_packaging"), b.long("packaging_fee"))
            val d = c.getValue("discount").jsonObject
            val discount = when (d.getValue("type").jsonPrimitive.content) {
                "percent" -> Discount.Percent(d.getValue("value").jsonPrimitive.double)
                "fixed" -> Discount.Fixed(d.long("value"))
                else -> Discount.None
            }
            val got = calculate(billing, c.long("subtotal"), OrderType.parse(c.getValue("order_type").jsonPrimitive.content)!!, discount, c.long("shipping_fee"), c.long("points_used"))
            val e = c.getValue("expected").jsonObject
            val exp = Totals(e.long("subtotal"), e.long("discount_amount"), e.long("tax_amount"), e.long("service_charge"),
                e.long("packaging_fee"), e.long("shipping_fee"), e.long("points_discount"), e.long("total"))
            if (got == exp) continue

            val tie = discount is Discount.Percent &&
                (c.long("subtotal") * Math.round(discount.value.coerceIn(0.0, 100.0) * 100.0)) % 10_000 == 5_000L
            if (tie && got.discountAmount == exp.discountAmount + 1) ties++ else bad += "${c.getValue("name")}: got=$got exp=$exp"
        }
        assertTrue(bad.joinToString("\n"), bad.isEmpty())
        assertTrue("terlalu banyak tie: $ties", ties <= 8)
    }

    @Test fun orderTypeAliases() {
        listOf("take_away", "takeaway", "Take Away", "take-away").forEach { assertEquals(OrderType.TAKE_AWAY, OrderType.parse(it)) }
        assertNull(OrderType.parse("ngawur"))
    }

    @Test fun worksheetFromBackendIntegrationTest() {

        val t = calculate(Billing(), 80_000, OrderType.DINE_IN, Discount.Percent(10.0))
        assertEquals(82_800L, t.total)
        assertEquals(32_800L, calculate(Billing(), 28_000, OrderType.TAKE_AWAY).total)
    }

    @Test fun rupiahFormat() {
        assertEquals("Rp25.000", rupiah(25_000)); assertEquals("-Rp600", rupiah(-600)); assertEquals("Rp0", rupiah(0))
    }
}
