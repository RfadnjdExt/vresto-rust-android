package id.vresto.pos.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import id.vresto.pos.core.Billing
import id.vresto.pos.core.Discount
import id.vresto.pos.core.OrderType
import id.vresto.pos.core.Totals
import id.vresto.pos.core.calculate
import id.vresto.pos.core.formatLocal
import id.vresto.pos.data.*
import id.vresto.pos.print.BluetoothPrinter
import id.vresto.pos.print.EscPos
import id.vresto.pos.print.Receipt
import id.vresto.pos.print.ReceiptLine
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.util.UUID
import javax.inject.Inject

data class PosUi(
    val loaded: Boolean = false,
    val busy: Boolean = false,
    val connected: Boolean = false,
    val sessionExpired: Boolean = false,
    val user: UserDto? = null,
    val products: List<ProductDto> = emptyList(),
    val categories: List<CategoryDto> = emptyList(),
    val tables: List<TableDto> = emptyList(),
    val orders: List<OrderDto> = emptyList(),
    val shift: ShiftSummaryDto? = null,
    val expenses: List<ExpenseDto> = emptyList(),
    val billing: Billing = Billing(),
    val shop: ShopDto = ShopDto(),
    val paymentMethods: List<PaymentMethodDto> = emptyList(),
    val customers: List<CustomerDto> = emptyList(),
    val stats: StatsDto = StatsDto(),
    val refundRequiresAdmin: Boolean = true,
)

data class CartLine(
    val key: Long,
    val product: ProductDto,
    val variant: VariantDto?,
    val modifiers: List<Pair<ModifierDto, Int>>,
    val qty: Int,
) {
    val unitPrice: Long get() = product.effectivePrice + (variant?.priceAdjustment ?: 0) + modifiers.sumOf { (m, q) -> m.price * q }
    val lineTotal: Long get() = unitPrice * qty
    val title: String get() = product.name + (variant?.let { " (${it.name})" } ?: "")
    val extras: List<String> get() = modifiers.map { (m, q) -> "+ ${m.name}" + if (q > 1) " x$q" else "" }
}

data class CartState(
    val lines: List<CartLine> = emptyList(),
    val orderType: OrderType = OrderType.DINE_IN,
    val tableId: Long? = null,
    val customerId: Long? = null,
    val discount: Discount = Discount.None,
) {
    val subtotal: Long get() = lines.sumOf { it.lineTotal }
    val count: Int get() = lines.sumOf { it.qty }
}

data class PayTarget(val orderId: Long?, val total: Long, val attemptId: String = UUID.randomUUID().toString())

data class ReceiptView(val receipt: Receipt, val pay: PayResultDto)

@OptIn(FlowPreview::class)
@HiltViewModel
class PosViewModel @Inject constructor(
    private val repo: PosRepository,
    private val realtime: RealtimeClient,
    private val printer: BluetoothPrinter,
    private val session: Session,
) : ViewModel() {

    private val _ui = MutableStateFlow(PosUi())
    val ui: StateFlow<PosUi> = _ui.asStateFlow()
    private val _cart = MutableStateFlow(CartState())
    val cart: StateFlow<CartState> = _cart.asStateFlow()
    private val _pay = MutableStateFlow<PayTarget?>(null)
    val pay: StateFlow<PayTarget?> = _pay.asStateFlow()
    private val _receipt = MutableStateFlow<ReceiptView?>(null)
    val receipt: StateFlow<ReceiptView?> = _receipt.asStateFlow()
    private val _messages = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val messages: SharedFlow<String> = _messages.asSharedFlow()

    private var lineSeq = 0L

    init {
        realtime.start()
        viewModelScope.launch {
            realtime.events.collect { e ->
                when (e.name) {
                    "connected" -> _ui.update { it.copy(connected = true) }
                    "disconnected" -> _ui.update { it.copy(connected = false) }
                }
            }
        }

        viewModelScope.launch {
            realtime.events.filter { it.name in REFRESH_ON }.debounce(300).collect { refreshLive() }
        }
        load()
    }

    fun totalsFor(c: CartState, billing: Billing): Totals = calculate(billing, c.subtotal, c.orderType, c.discount)

    private fun toast(m: String) { _messages.tryEmit(m) }

    private fun launch(block: suspend () -> Unit) = viewModelScope.launch {
        _ui.update { it.copy(busy = true) }
        try { block() }
        catch (e: ApiException) {
            if (e.isUnauthorized) _ui.update { it.copy(sessionExpired = true) } else toast(e.message ?: "Terjadi kesalahan.")
        } finally { _ui.update { it.copy(busy = false) } }
    }

    fun load() = launch {
        val d = repo.init()
        _ui.update {
            it.copy(
                loaded = true, user = d.user, products = d.products, categories = d.categories, tables = d.tables,
                orders = d.activeOrders, shift = d.currentShift, stats = d.stats, shop = d.shop,
                paymentMethods = d.paymentMethods, customers = d.customers, refundRequiresAdmin = d.billing.refundRequiresAdmin,
                billing = Billing.fromRates(d.billing.taxRate, d.billing.applyTax, d.billing.serviceChargeRate,
                    d.billing.applyServiceChargeDineIn, d.billing.packagingFee, d.billing.applyPackagingFee),
            )
        }
    }

    private suspend fun refreshLive() = try {
        coroutineScope {
            val orders = async { repo.activeOrders() }
            val tables = async { repo.tables() }
            val stats = async { repo.stats() }
            val shift = async { repo.currentShift() }
            _ui.update { it.copy(orders = orders.await(), tables = tables.await(), stats = stats.await(), shift = shift.await()) }
        }
    } catch (e: ApiException) {
        if (e.isUnauthorized) _ui.update { it.copy(sessionExpired = true) }
    }

    fun refresh() { viewModelScope.launch { refreshLive() } }

    fun logout() {
        realtime.stop()
        viewModelScope.launch { repo.logout() }
    }

    fun addToCart(p: ProductDto, variant: VariantDto? = null, mods: List<Pair<ModifierDto, Int>> = emptyList()) {
        _cart.update { c ->

            val sig = { l: CartLine -> Triple(l.product.id, l.variant?.id, l.modifiers.map { it.first.id to it.second }) }
            val probe = CartLine(0, p, variant, mods, 1)
            val i = c.lines.indexOfFirst { sig(it) == sig(probe) }
            if (i >= 0) c.copy(lines = c.lines.toMutableList().also { it[i] = it[i].copy(qty = it[i].qty + 1) })
            else c.copy(lines = c.lines + probe.copy(key = ++lineSeq))
        }
    }

    fun changeQty(key: Long, delta: Int) = _cart.update { c ->
        c.copy(lines = c.lines.mapNotNull { if (it.key == key) it.copy(qty = it.qty + delta).takeIf { l -> l.qty > 0 } else it })
    }
    fun removeLine(key: Long) = _cart.update { c -> c.copy(lines = c.lines.filterNot { it.key == key }) }
    fun clearCart() = _cart.update { CartState(orderType = it.orderType) }
    fun setOrderType(t: OrderType) = _cart.update { it.copy(orderType = t, tableId = if (t == OrderType.DINE_IN || t == OrderType.KARAOKE) it.tableId else null) }
    fun setTable(id: Long?) = _cart.update { it.copy(tableId = id) }
    fun setCustomer(id: Long?) = _cart.update { it.copy(customerId = id) }
    fun setDiscount(d: Discount) = _cart.update { it.copy(discount = d) }

    private fun orderReq(c: CartState) = OrderReq(
        items = c.lines.map { l ->
            CartItemReq(l.product.id, l.variant?.id, l.qty, null, l.modifiers.map { (m, q) -> ModifierReq(m.id, q) })
        },
        orderType = c.orderType.wire,
        tableId = c.tableId,
        customerId = c.customerId,
        discountType = when (c.discount) { is Discount.Percent -> "percent"; is Discount.Fixed -> "fixed"; else -> null },
        discountValue = when (val d = c.discount) { is Discount.Percent -> d.value; is Discount.Fixed -> d.value.toDouble(); else -> null },
    )

    fun saveOrder() = launch {
        val c = _cart.value
        if (c.lines.isEmpty()) return@launch toast("Keranjang kosong.")
        val o = repo.createOrder(orderReq(c))
        clearCart()
        toast("Pesanan ${o.orderNumber} disimpan.")
        refreshLive()
    }

    fun openPaymentForCart() {
        val c = _cart.value
        if (c.lines.isEmpty()) return toast("Keranjang kosong.")
        if (_ui.value.shift == null) return toast("Buka shift dulu di tab Shift.")
        _pay.value = PayTarget(null, totalsFor(c, _ui.value.billing).total)
    }

    fun openPaymentForOrder(o: OrderDto) {
        if (_ui.value.shift == null) return toast("Buka shift dulu di tab Shift.")
        _pay.value = PayTarget(o.id, o.totalAmount)
    }

    fun dismissPayment() { _pay.value = null }

    fun confirmPay(method: String, paid: Long, customerId: Long?) = launch {
        val t = _pay.value ?: return@launch
        val c = _cart.value
        val req = PayReq(
            clientUuid = t.attemptId,
            orderId = t.orderId,
            order = if (t.orderId == null) orderReq(c) else null,
            paymentMethod = method,
            amountPaid = paid,
            customerId = customerId ?: c.customerId.takeIf { t.orderId == null },
        )
        val res = repo.pay(req)
        val order = repo.order(res.orderId)
        _pay.value = null
        if (t.orderId == null) clearCart()
        _receipt.value = ReceiptView(buildReceipt(order, res), res)
        refreshLive()
    }

    fun closeReceipt() { _receipt.value = null }

    private fun buildReceipt(o: OrderDto, pay: PayResultDto): Receipt {
        val shop = _ui.value.shop
        return Receipt(
            shopName = shop.name, address = shop.address, phone = shop.phone,
            orderNumber = o.orderNumber, queueNumber = o.queueNumber,
            orderTypeLabel = OrderType.parse(o.orderType)?.label ?: o.orderType,
            tableName = o.tableName, cashier = _ui.value.user?.name,
            dateTime = formatLocal(pay.paidAt ?: o.createdAt),
            lines = o.items.map { i -> ReceiptLine(i.productName, i.quantity, i.totalPrice, i.modifiers.map { m -> "+ ${m.name}" + if (m.quantity > 1) " x${m.quantity}" else "" }) },
            subtotal = o.subtotal, discount = o.discountAmount, tax = o.taxAmount, service = o.serviceCharge,
            packaging = o.packagingFee, shipping = o.shippingFee, total = o.totalAmount,
            paymentLabel = _ui.value.paymentMethods.firstOrNull { it.id == pay.paymentMethod }?.name ?: pay.paymentMethod,
            paid = pay.amountPaid, change = pay.changeAmount,
        )
    }

    fun print(r: Receipt) = viewModelScope.launch {
        printer.print(EscPos.render(r))
            .onSuccess { toast("Struk dicetak.") }
            .onFailure { toast(it.message ?: "Gagal mencetak.") }
    }

    fun setStatus(id: Long, status: String) = launch { repo.setStatus(id, status); refreshLive() }
    fun completeOrder(id: Long) = launch { repo.complete(id); toast("Pesanan selesai."); refreshLive() }
    fun cancelOrder(id: Long) = launch { repo.cancel(id); toast("Pesanan dibatalkan."); refreshLive() }

    fun startShift(openingCash: Long) = launch { val s = repo.startShift(openingCash, null); _ui.update { u -> u.copy(shift = s) } }
    fun closeShift(actual: Long, notes: String?, onDone: (ShiftSummaryDto) -> Unit) = launch {
        val s = repo.closeShift(actual, notes)
        _ui.update { it.copy(shift = null, expenses = emptyList()) }
        onDone(s)
    }
    fun cashMove(type: String, amount: Long, desc: String?) = launch { _ui.update { it.copy(shift = repo.cashMove(type, amount, desc)) } }
    fun loadExpenses() = launch { _ui.update { it.copy(expenses = repo.expenses()) } }
    fun addExpense(category: String, desc: String?, amount: Long) = launch {
        repo.addExpense(category, desc, amount)
        _ui.update { it.copy(expenses = repo.expenses(), shift = repo.currentShift()) }
    }

    fun pairedPrinters() = printer.pairedDevices()
    fun hasBluetoothPermission() = printer.hasPermission()
    fun selectedPrinter(): String = session.snapshot.printerMac
    fun selectPrinter(mac: String) { viewModelScope.launch { session.savePrinter(mac) } }

    private companion object { val REFRESH_ON = setOf("order.updated", "table.updated", "stats.updated", "resync") }
}
