package id.vresto.pos.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class Envelope<T>(val success: Boolean = true, val message: String? = null, val data: T? = null)

@Serializable
data class ErrorBody(val message: String? = null, val code: String? = null)

@Serializable data class LoginReq(val username: String, val password: String)
@Serializable data class UserDto(val id: Long, val name: String, val role: String) {
    val isAdmin get() = role == "admin"
}
@Serializable data class LoginData(val token: String, val user: UserDto)

@Serializable data class VariantDto(
    val id: Long, val name: String,
    @SerialName("price_adjustment") val priceAdjustment: Long = 0,
)

@Serializable data class ModifierDto(
    val id: Long, val name: String, val price: Long = 0,
    @SerialName("max_quantity") val maxQuantity: Int = 1,
    @SerialName("is_required") val isRequired: Boolean = false,
)

@Serializable data class ProductDto(
    val id: Long,
    val name: String,
    @SerialName("category_id") val categoryId: Long? = null,
    val price: Long,
    @SerialName("effective_price") val effectivePrice: Long = price,
    val type: String = "single",
    @SerialName("is_stock_managed") val isStockManaged: Boolean = false,
    @SerialName("stock_quantity") val stockQuantity: Long = 0,
    @SerialName("is_low_stock") val isLowStock: Boolean = false,
    val variants: List<VariantDto> = emptyList(),
    val modifiers: List<ModifierDto> = emptyList(),
) {
    val hasOptions get() = variants.isNotEmpty() || modifiers.isNotEmpty()
}

@Serializable data class CategoryDto(val id: Long, val name: String)

@Serializable data class TableDto(
    val id: Long, val name: String, val section: String? = null, val capacity: Int = 4,
    val status: String = "available",
    @SerialName("current_order_id") val currentOrderId: Long? = null,
    @SerialName("current_order_number") val currentOrderNumber: String? = null,
)

@Serializable data class PaymentMethodDto(val id: String, val name: String)

@Serializable data class CustomerDto(
    val id: Long, val name: String, val phone: String? = null, val points: Long = 0,
    @SerialName("credit_limit") val creditLimit: Long = 0,
    @SerialName("total_debt") val totalDebt: Long = 0,
)

@Serializable data class BillingDto(
    @SerialName("tax_rate") val taxRate: Double = 0.10,
    @SerialName("apply_tax") val applyTax: Boolean = true,
    @SerialName("service_charge_rate") val serviceChargeRate: Double = 0.05,
    @SerialName("apply_service_charge_dine_in") val applyServiceChargeDineIn: Boolean = true,
    @SerialName("packaging_fee") val packagingFee: Long = 2000,
    @SerialName("apply_packaging_fee") val applyPackagingFee: Boolean = true,

    @SerialName("refund_requires_admin") val refundRequiresAdmin: Boolean = true,
)

@Serializable data class ShopDto(
    val name: String = "Vresto", val address: String = "", val phone: String = "",
    @SerialName("business_type") val businessType: String? = null,
)

@Serializable data class OrderModifierDto(
    @SerialName("modifier_name") val name: String, val quantity: Int = 1,
    @SerialName("total_price") val totalPrice: Long = 0,
)

@Serializable data class OrderItemDto(
    val id: Long,
    @SerialName("product_name") val productName: String,
    val quantity: Int,
    @SerialName("unit_price") val unitPrice: Long = 0,
    @SerialName("total_price") val totalPrice: Long,
    val note: String? = null,
    val modifiers: List<OrderModifierDto> = emptyList(),
)

@Serializable data class OrderDto(
    val id: Long,
    @SerialName("order_number") val orderNumber: String,
    @SerialName("order_type") val orderType: String = "dine_in",
    val status: String,
    @SerialName("payment_status") val paymentStatus: String = "unpaid",
    @SerialName("queue_number") val queueNumber: Int? = null,
    @SerialName("table_name") val tableName: String? = null,
    @SerialName("customer_display") val customerDisplay: String? = null,
    val subtotal: Long = 0,
    @SerialName("discount_amount") val discountAmount: Long = 0,
    @SerialName("tax_amount") val taxAmount: Long = 0,
    @SerialName("service_charge") val serviceCharge: Long = 0,
    @SerialName("packaging_fee") val packagingFee: Long = 0,
    @SerialName("shipping_fee") val shippingFee: Long = 0,
    @SerialName("total_amount") val totalAmount: Long,
    @SerialName("created_at") val createdAt: String = "",
    val items: List<OrderItemDto> = emptyList(),
) {
    val isPaid get() = paymentStatus == "paid"
}

@Serializable data class ModifierReq(val id: Long, val quantity: Int = 1)

@Serializable data class CartItemReq(
    @SerialName("product_id") val productId: Long,
    @SerialName("variant_id") val variantId: Long? = null,
    val qty: Int,
    val note: String? = null,
    val modifiers: List<ModifierReq> = emptyList(),
)

@Serializable data class OrderReq(
    val items: List<CartItemReq>,
    @SerialName("order_type") val orderType: String,
    @SerialName("table_id") val tableId: Long? = null,
    @SerialName("customer_id") val customerId: Long? = null,
    @SerialName("discount_type") val discountType: String? = null,
    @SerialName("discount_value") val discountValue: Double? = null,
    val notes: String? = null,
)

@Serializable data class PayReq(

    @SerialName("client_uuid") val clientUuid: String,
    @SerialName("order_id") val orderId: Long? = null,
    val order: OrderReq? = null,
    @SerialName("payment_method") val paymentMethod: String,
    @SerialName("amount_paid") val amountPaid: Long,
    @SerialName("customer_id") val customerId: Long? = null,
)

@Serializable data class PayResultDto(
    @SerialName("transaction_id") val transactionId: Long,
    @SerialName("trx_number") val trxNumber: String,
    @SerialName("order_id") val orderId: Long,
    @SerialName("order_number") val orderNumber: String,
    @SerialName("total_amount") val totalAmount: Long,
    @SerialName("amount_paid") val amountPaid: Long,
    @SerialName("change_amount") val changeAmount: Long = 0,
    @SerialName("payment_method") val paymentMethod: String,
    @SerialName("paid_at") val paidAt: String? = null,
)

@Serializable data class StatusReq(val status: String)

@Serializable data class ShiftDto(
    val id: Long,
    @SerialName("shift_number") val shiftNumber: String,
    val status: String,
    @SerialName("opening_cash") val openingCash: Long = 0,
    @SerialName("closing_cash") val closingCash: Long? = null,
    @SerialName("cash_difference") val cashDifference: Long? = null,
)

@Serializable data class SalesDto(
    @SerialName("total_sales") val totalSales: Long = 0,
    @SerialName("cash_sales") val cashSales: Long = 0,
    @SerialName("digital_sales") val digitalSales: Long = 0,
    @SerialName("total_orders") val totalOrders: Long = 0,
)

@Serializable data class ShiftSummaryDto(
    val shift: ShiftDto,
    val sales: SalesDto = SalesDto(),
    @SerialName("total_expenses") val totalExpenses: Long = 0,
    @SerialName("expected_cash") val expectedCash: Long = 0,

    @SerialName("debt_collected") val debtCollected: DebtCollectedDto = DebtCollectedDto(),
)

@Serializable data class DebtCollectedDto(val cash: Long = 0, val digital: Long = 0, val total: Long = 0)

@Serializable data class StartShiftReq(@SerialName("opening_cash") val openingCash: Long, val notes: String? = null)
@Serializable data class CloseShiftReq(@SerialName("actual_cash") val actualCash: Long, val notes: String? = null)
@Serializable data class CashMoveReq(val type: String, val amount: Long, val description: String? = null)
@Serializable data class ExpenseReq(val category: String, val description: String? = null, val amount: Long)
@Serializable data class ExpenseDto(val id: Long, val category: String, val description: String? = null, val amount: Long)

@Serializable data class StatsDto(
    @SerialName("today_sales") val todaySales: Long = 0,
    @SerialName("today_transactions") val todayTransactions: Long = 0,
    @SerialName("active_orders") val activeOrders: Long = 0,
    @SerialName("receivables_total") val receivablesTotal: Long = 0,
)

@Serializable data class InitDto(
    val user: UserDto,
    val products: List<ProductDto> = emptyList(),
    val categories: List<CategoryDto> = emptyList(),
    val tables: List<TableDto> = emptyList(),
    @SerialName("active_orders") val activeOrders: List<OrderDto> = emptyList(),
    @SerialName("current_shift") val currentShift: ShiftSummaryDto? = null,
    val stats: StatsDto = StatsDto(),
    @SerialName("billing_settings") val billing: BillingDto = BillingDto(),
    @SerialName("shop_settings") val shop: ShopDto = ShopDto(),
    @SerialName("payment_methods") val paymentMethods: List<PaymentMethodDto> = emptyList(),
    val customers: List<CustomerDto> = emptyList(),
)

@Serializable data class TransactionDto(
    val id: Long,
    @SerialName("trx_number") val trxNumber: String,
    @SerialName("order_id") val orderId: Long? = null,
    @SerialName("order_number") val orderNumber: String? = null,
    @SerialName("order_type") val orderType: String? = null,
    @SerialName("order_status") val orderStatus: String? = null,
    @SerialName("customer_id") val customerId: Long? = null,
    @SerialName("customer_name") val customerName: String? = null,
    val cashier: String? = null,
    @SerialName("user_id") val userId: Long = 0,
    @SerialName("shift_id") val shiftId: Long? = null,
    @SerialName("payment_method") val paymentMethod: String,
    @SerialName("total_amount") val totalAmount: Long,
    @SerialName("amount_paid") val amountPaid: Long = 0,
    @SerialName("change_amount") val changeAmount: Long = 0,
    @SerialName("refunded_amount") val refundedAmount: Long = 0,
    @SerialName("remaining_refundable") val remainingRefundable: Long = 0,
    val status: String,
    @SerialName("paid_at") val paidAt: String? = null,
    @SerialName("created_at") val createdAt: String = "",
    val refunds: List<RefundDto> = emptyList(),
    val void: VoidInfoDto? = null,
)

@Serializable data class RefundDto(
    val id: Long, val amount: Long,
    @SerialName("payment_method") val paymentMethod: String,
    val reason: String,
    @SerialName("is_restocked") val isRestocked: Boolean = false,
    @SerialName("shift_id") val shiftId: Long? = null,
    @SerialName("created_at") val createdAt: String = "",
    val user: String? = null,
)

@Serializable data class VoidInfoDto(val reason: String, @SerialName("created_at") val createdAt: String = "", val user: String? = null)

@Serializable data class TrxSummaryDto(
    @SerialName("total_sales") val totalSales: Long = 0,
    @SerialName("total_transactions") val totalTransactions: Long = 0,
    @SerialName("average_transaction") val averageTransaction: Long = 0,
    @SerialName("cancelled_count") val cancelledCount: Long = 0,
)

@Serializable data class TransactionListResponse(
    val success: Boolean = true,
    val data: List<TransactionDto> = emptyList(),
    val summary: TrxSummaryDto = TrxSummaryDto(),
)

@Serializable data class VoidReq(val reason: String)

@Serializable data class RefundReq(

    @SerialName("client_uuid") val clientUuid: String,
    val amount: Long,
    val reason: String,
    val restock: Boolean = false,
)

@Serializable data class RefundInfoDto(
    val id: Long,
    @SerialName("transaction_id") val transactionId: Long,
    val amount: Long,
    @SerialName("payment_method") val paymentMethod: String,
    @SerialName("shift_id") val shiftId: Long? = null,
    val reason: String,
    @SerialName("is_restocked") val isRestocked: Boolean = false,
    @SerialName("created_at") val createdAt: String = "",
    @SerialName("transaction_status") val transactionStatus: String,
    @SerialName("total_amount") val totalAmount: Long,
    @SerialName("refunded_amount") val refundedAmount: Long,
    @SerialName("remaining_refundable") val remainingRefundable: Long,
)

@Serializable data class RefundResultDto(val refund: RefundInfoDto, @SerialName("cash_out") val cashOut: Boolean = false)

@Serializable data class DebtPaymentDto(
    val id: Long, val amount: Long,
    @SerialName("payment_method") val paymentMethod: String,
    val notes: String? = null,
    @SerialName("created_at") val createdAt: String = "",
    @SerialName("user_id") val userId: Long = 0,
    @SerialName("shift_id") val shiftId: Long? = null,
    val cashier: String? = null,
    @SerialName("is_reversed") val isReversed: Boolean = false,
    @SerialName("reversal_reason") val reversalReason: String? = null,
    @SerialName("reversed_at") val reversedAt: String? = null,
)

@Serializable data class DebtDto(
    val id: Long,
    @SerialName("customer_id") val customerId: Long,
    @SerialName("customer_name") val customerName: String,
    @SerialName("order_id") val orderId: Long? = null,
    @SerialName("order_number") val orderNumber: String? = null,
    @SerialName("transaction_id") val transactionId: Long? = null,
    val amount: Long,
    @SerialName("remaining_amount") val remainingAmount: Long,
    @SerialName("due_date") val dueDate: String? = null,
    val status: String,
    val notes: String? = null,
    @SerialName("created_at") val createdAt: String = "",
    @SerialName("is_overdue") val isOverdue: Boolean = false,
    val payments: List<DebtPaymentDto> = emptyList(),
)

@Serializable data class RepayReq(
    @SerialName("client_uuid") val clientUuid: String,
    val amount: Long,
    @SerialName("payment_method") val paymentMethod: String,
    val notes: String? = null,
)

@Serializable data class RepayPartDto(
    val id: Long,
    @SerialName("debt_id") val debtId: Long,
    val amount: Long,
    @SerialName("remaining_after") val remainingAfter: Long,
    @SerialName("debt_status") val debtStatus: String,
    @SerialName("shift_id") val shiftId: Long? = null,
)

@Serializable data class DebtCustomerDto(
    val id: Long, val name: String,
    @SerialName("total_debt") val totalDebt: Long = 0,
    @SerialName("credit_limit") val creditLimit: Long = 0,
)

@Serializable data class RepayResultDto(
    val applied: Long,
    val payments: List<RepayPartDto> = emptyList(),
    val customer: DebtCustomerDto,
    @SerialName("payment_method") val paymentMethod: String,
)

@Serializable data class ReverseReq(val reason: String)

@Serializable data class ReverseResultDto(
    @SerialName("payment_id") val paymentId: Long,
    val amount: Long,
    @SerialName("payment_method") val paymentMethod: String,
    @SerialName("shift_id") val shiftId: Long? = null,
    val reason: String,
    @SerialName("reversed_at") val reversedAt: String = "",
    @SerialName("debt_id") val debtId: Long,
    @SerialName("debt_amount") val debtAmount: Long,
    @SerialName("remaining_amount") val remainingAmount: Long,
    @SerialName("debt_status") val debtStatus: String,
    @SerialName("customer_id") val customerId: Long,
    @SerialName("customer_name") val customerName: String,
    @SerialName("total_debt") val totalDebt: Long = 0,
)
