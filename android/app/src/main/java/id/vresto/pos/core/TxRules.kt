package id.vresto.pos.core

object TxRules {
    fun statusLabel(status: String): String = when (status) {
        "confirmed" -> "Lunas"
        "partial_refund" -> "Refund sebagian"
        "refunded" -> "Direfund"
        "voided" -> "Void"
        "unpaid" -> "Hutang"
        "pending" -> "Menunggu"
        "failed" -> "Gagal"
        "cancelled" -> "Batal"
        else -> status
    }

    fun debtStatusLabel(status: String): String = when (status) {
        "OPEN" -> "Belum dibayar"
        "PARTIAL" -> "Dicicil"
        "PAID" -> "Lunas"
        "VOID" -> "Dibatalkan"
        else -> status
    }

    fun remainingRefundable(total: Long, refunded: Long): Long = maxOf(0L, total - refunded)

    fun canVoid(
        status: String, orderStatus: String?, refunded: Long,
        trxShiftId: Long?, trxUserId: Long,
        myShiftId: Long?, myUserId: Long, isAdmin: Boolean,
    ): Boolean {
        if (refunded > 0) return false
        if (orderStatus == "completed" || orderStatus == "cancelled") return false
        return when (status) {
            "unpaid" -> isAdmin || trxUserId == myUserId
            "confirmed" -> trxShiftId != null && (isAdmin || trxShiftId == myShiftId)
            else -> false
        }
    }

    fun canRefund(status: String, remaining: Long, isAdmin: Boolean, refundRequiresAdmin: Boolean): Boolean =
        (status == "confirmed" || status == "partial_refund") && remaining > 0 && (isAdmin || !refundRequiresAdmin)

    fun mustRefundFully(paymentMethod: String): Boolean = paymentMethod == "points"

    fun canRestock(amount: Long, remaining: Long, orderStatus: String?): Boolean =
        amount > 0 && amount == remaining && orderStatus == "completed"

    fun validateRefundAmount(amount: Long, remaining: Long): String? = when {
        amount <= 0 -> "Jumlah refund harus lebih dari 0."
        amount > remaining -> "Jumlah melebihi sisa yang bisa direfund (${rupiah(remaining)})."
        else -> null
    }

    fun canRepay(debtStatus: String, remaining: Long): Boolean = (debtStatus == "OPEN" || debtStatus == "PARTIAL") && remaining > 0

    fun validateRepayAmount(amount: Long, remaining: Long): String? = when {
        amount <= 0 -> "Jumlah pembayaran harus lebih dari 0."
        amount > remaining -> "Jumlah melebihi sisa hutang (${rupiah(remaining)})."
        else -> null
    }

    fun repayMethods(all: List<String>): List<String> = all.filter { it != "debt" && it != "points" }

    fun canReversePayment(
        isReversed: Boolean, isAdmin: Boolean,
        paymentUserId: Long, paymentShiftId: Long?, myUserId: Long, myShiftId: Long?,
    ): Boolean = !isReversed && (isAdmin || (paymentShiftId != null && paymentShiftId == myShiftId && paymentUserId == myUserId))
}
