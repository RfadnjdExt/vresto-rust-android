package id.vresto.pos.core

import kotlin.math.roundToLong

enum class OrderType(val wire: String, val label: String) {
    DINE_IN("dine_in", "Dine In"),
    TAKE_AWAY("take_away", "Take Away"),
    DELIVERY("delivery", "Delivery"),
    GOFOOD("gofood", "GoFood"),
    GRABFOOD("grabfood", "GrabFood"),
    SHOPEEFOOD("shopeefood", "ShopeeFood"),
    KARAOKE("karaoke", "Karaoke");

    val paysPackaging: Boolean get() = this == TAKE_AWAY || this == DELIVERY

    companion object {

        fun parse(s: String): OrderType? {
            val n = s.trim().lowercase().replace(' ', '_').replace('-', '_')
            return when (n) {
                "dine_in", "dinein" -> DINE_IN
                "take_away", "takeaway" -> TAKE_AWAY
                else -> entries.firstOrNull { it.wire == n }
            }
        }
    }
}

data class Billing(
    val applyTax: Boolean = true,
    val taxBps: Long = 1000,
    val applyServiceDineIn: Boolean = true,
    val serviceBps: Long = 500,
    val applyPackaging: Boolean = true,
    val packagingFee: Long = 2000,
) {
    companion object {

        fun fromRates(
            taxRate: Double, applyTax: Boolean, serviceRate: Double, applyService: Boolean,
            packagingFee: Long, applyPackaging: Boolean,
        ) = Billing(applyTax, rateToBps(taxRate), applyService, rateToBps(serviceRate), applyPackaging, packagingFee)
    }
}

sealed interface Discount {
    data object None : Discount
    data class Fixed(val value: Long) : Discount

    data class Percent(val value: Double) : Discount
}

data class Totals(
    val subtotal: Long,
    val discountAmount: Long,
    val taxAmount: Long,
    val serviceCharge: Long,
    val packagingFee: Long,
    val shippingFee: Long,
    val pointsDiscount: Long,
    val total: Long,
)

const val POINT_VALUE = 100L

fun rateToBps(rate: Double): Long = (rate * 10_000.0).roundToLong()

fun applyBps(amount: Long, bps: Long): Long = (amount * bps + 5_000L) / 10_000L

fun calculate(
    b: Billing,
    subtotal: Long,
    type: OrderType,
    discount: Discount = Discount.None,
    shippingFee: Long = 0,
    pointsUsed: Long = 0,
): Totals {
    val sub = maxOf(0L, subtotal)
    val rawDiscount = when (discount) {
        Discount.None -> 0L
        is Discount.Fixed -> maxOf(0L, discount.value)
        is Discount.Percent -> {
            val hundredths = (discount.value.coerceIn(0.0, 100.0) * 100.0).roundToLong()
            (sub * hundredths + 5_000L) / 10_000L
        }
    }
    val disc = minOf(rawDiscount, sub)
    val base = sub - disc
    val tax = if (b.applyTax) applyBps(base, b.taxBps) else 0L
    val service = if (type == OrderType.DINE_IN && b.applyServiceDineIn) applyBps(base, b.serviceBps) else 0L
    val packaging = if (type.paysPackaging && b.applyPackaging) b.packagingFee else 0L
    val shipping = maxOf(0L, shippingFee)
    val points = maxOf(0L, pointsUsed) * POINT_VALUE
    val total = maxOf(0L, sub - disc + tax + service + packaging + shipping - points)
    return Totals(sub, disc, tax, service, packaging, shipping, points, total)
}
