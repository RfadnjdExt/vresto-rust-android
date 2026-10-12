package id.vresto.pos.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import id.vresto.pos.core.formatLocal
import id.vresto.pos.core.rupiah
import id.vresto.pos.data.*
import id.vresto.pos.print.BluetoothPrinter
import id.vresto.pos.print.DebtSlip
import id.vresto.pos.print.EscPos
import id.vresto.pos.print.RefundSlip
import kotlinx.coroutines.FlowPreview
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
import java.time.LocalDate
import javax.inject.Inject

data class TxDetail(val trx: TransactionDto, val order: OrderDto?)

data class SlipContext(val shop: ShopDto, val cashier: String?, val methods: Map<String, String>) {
    fun label(id: String) = methods[id] ?: id
}

data class HistoryUi(
    val date: LocalDate = LocalDate.now(),
    val status: String = "all",
    val search: String = "",
    val loading: Boolean = false,
    val busy: Boolean = false,
    val trx: List<TransactionDto> = emptyList(),
    val summary: TrxSummaryDto = TrxSummaryDto(),
    val detail: TxDetail? = null,
    val refundDone: RefundSlip? = null,

    val debtFilter: String = "open",
    val debts: List<DebtDto> = emptyList(),
    val debtDetail: DebtDto? = null,

    val customerOpenDebts: List<DebtDto> = emptyList(),
    val repayDone: DebtSlip? = null,
) {
    val customerOpenTotal: Long get() = customerOpenDebts.sumOf { it.remainingAmount }
}

@OptIn(FlowPreview::class)
@HiltViewModel
class HistoryViewModel @Inject constructor(
    private val repo: PosRepository,
    private val realtime: RealtimeClient,
    private val printer: BluetoothPrinter,
) : ViewModel() {

    private val _ui = MutableStateFlow(HistoryUi())
    val ui: StateFlow<HistoryUi> = _ui.asStateFlow()
    private val _messages = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val messages: SharedFlow<String> = _messages.asSharedFlow()
    private val _expired = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    val sessionExpired: SharedFlow<Unit> = _expired.asSharedFlow()

    init {

        viewModelScope.launch {
            realtime.events.filter { it.name in REFRESH_ON }.debounce(500).collect { reloadQuiet() }
        }
    }

    private fun toast(m: String) { _messages.tryEmit(m) }
    private fun handle(e: ApiException) { if (e.isUnauthorized) _expired.tryEmit(Unit) else toast(e.message ?: "Terjadi kesalahan.") }

    private fun act(block: suspend () -> Unit) = viewModelScope.launch {
        _ui.update { it.copy(busy = true) }
        try { block() } catch (e: ApiException) { handle(e) } finally { _ui.update { it.copy(busy = false) } }
    }

    fun loadTransactions() = viewModelScope.launch {
        _ui.update { it.copy(loading = true) }
        try {
            val s = _ui.value
            val r = repo.transactions(s.date.toString(), s.date.toString(), s.status, s.search)
            _ui.update { it.copy(trx = r.data, summary = r.summary) }
        } catch (e: ApiException) { handle(e) }
        finally { _ui.update { it.copy(loading = false) } }
    }

    fun setDate(d: LocalDate) { _ui.update { it.copy(date = d) }; loadTransactions() }
    fun shiftDay(delta: Long) {
        val next = _ui.value.date.plusDays(delta)
        setDate(if (next.isAfter(LocalDate.now())) LocalDate.now() else next)
    }
    fun setStatus(s: String) { _ui.update { it.copy(status = s) }; loadTransactions() }
    fun setSearch(q: String) { _ui.update { it.copy(search = q) }; loadTransactions() }

    fun openTransaction(id: Long) = act {
        val t = repo.transaction(id)
        val o = try { t.orderId?.let { repo.order(it) } } catch (e: ApiException) { null }
        _ui.update { it.copy(detail = TxDetail(t, o)) }
    }
    fun closeTransaction() { _ui.update { it.copy(detail = null) } }

    fun voidTransaction(id: Long, reason: String) = act {
        repo.voidTransaction(id, reason.trim())
        toast("Transaksi di-void.")
        reloadQuiet(); reopenDetail(id)
    }

    fun refund(trx: TransactionDto, amount: Long, reason: String, restock: Boolean, attemptId: String, ctx: SlipContext) = act {
        val res = repo.refund(trx.id, RefundReq(attemptId, amount, reason.trim(), restock))
        val r = res.refund
        val slip = RefundSlip(
            shopName = ctx.shop.name, address = ctx.shop.address, phone = ctx.shop.phone,
            trxNumber = trx.trxNumber, orderNumber = trx.orderNumber, dateTime = formatLocal(r.createdAt), cashier = ctx.cashier,
            reason = r.reason, amount = r.amount, methodLabel = ctx.label(r.paymentMethod), cashOut = res.cashOut,
            totalRefunded = r.refundedAmount, remaining = r.remainingRefundable, isFull = r.remainingRefundable == 0L,
        )
        _ui.update { it.copy(refundDone = slip) }
        reloadQuiet(); reopenDetail(trx.id)
    }
    fun dismissRefundDone() { _ui.update { it.copy(refundDone = null) } }

    private suspend fun reopenDetail(id: Long) {
        if (_ui.value.detail?.trx?.id != id) return
        try {
            val t = repo.transaction(id)
            val o = try { t.orderId?.let { repo.order(it) } } catch (e: ApiException) { null }
            _ui.update { it.copy(detail = TxDetail(t, o)) }
        } catch (e: ApiException) { handle(e) }
    }

    fun loadDebts() = viewModelScope.launch {
        try { _ui.update { it.copy(debts = repo.debts(it.debtFilter)) } } catch (e: ApiException) { handle(e) }
    }
    fun setDebtFilter(f: String) { _ui.update { it.copy(debtFilter = f) }; loadDebts() }

    fun openDebt(id: Long) = act { loadDebtDetail(id) }
    fun closeDebt() { _ui.update { it.copy(debtDetail = null, customerOpenDebts = emptyList()) } }

    private suspend fun loadDebtDetail(id: Long) {
        val d = repo.debt(id)
        val open = repo.debts("open", d.customerId)
        _ui.update { it.copy(debtDetail = d, customerOpenDebts = open) }
    }

    fun repay(debt: DebtDto, amount: Long, method: String, notes: String?, wholeCustomer: Boolean, attemptId: String, ctx: SlipContext) = act {
        val req = RepayReq(attemptId, amount, method, notes?.trim()?.takeIf { it.isNotEmpty() })
        val res = if (wholeCustomer) repo.repayCustomer(debt.customerId, req) else repo.repayDebt(debt.id, req)
        val known = _ui.value.customerOpenDebts + debt
        val slip = DebtSlip(
            shopName = ctx.shop.name, address = ctx.shop.address, phone = ctx.shop.phone,
            customerName = res.customer.name,
            orderNumbers = res.payments.mapNotNull { p -> known.firstOrNull { it.id == p.debtId }?.orderNumber }.distinct(),
            dateTime = formatLocal(java.time.OffsetDateTime.now().toString()), cashier = ctx.cashier,
            amount = res.applied, methodLabel = ctx.label(res.paymentMethod),

            remainingAfter = res.customer.totalDebt, notes = notes?.trim()?.takeIf { it.isNotEmpty() },
        )
        _ui.update { it.copy(repayDone = slip) }
        loadDebtDetail(debt.id)
        _ui.update { it.copy(debts = repo.debts(it.debtFilter)) }
    }
    fun dismissRepayDone() { _ui.update { it.copy(repayDone = null) } }

    fun reversePayment(paymentId: Long, reason: String) = act {
        val r = repo.reversePayment(paymentId, reason.trim())
        toast("Pelunasan dibatalkan. Sisa hutang ${rupiah(r.remainingAmount)}.")
        loadDebtDetail(r.debtId)
        _ui.update { it.copy(debts = repo.debts(it.debtFilter)) }
    }

    fun printRefund(s: RefundSlip) = viewModelScope.launch {
        printer.print(EscPos.renderRefund(s)).onSuccess { toast("Bukti refund dicetak.") }.onFailure { toast(it.message ?: "Gagal mencetak.") }
    }
    fun printDebt(s: DebtSlip) = viewModelScope.launch {
        printer.print(EscPos.renderDebtPayment(s)).onSuccess { toast("Bukti pelunasan dicetak.") }.onFailure { toast(it.message ?: "Gagal mencetak.") }
    }

    private suspend fun reloadQuiet() {
        try {
            val s = _ui.value
            val r = repo.transactions(s.date.toString(), s.date.toString(), s.status, s.search)
            val d = repo.debts(s.debtFilter)
            _ui.update { it.copy(trx = r.data, summary = r.summary, debts = d) }
        } catch (e: ApiException) { if (e.isUnauthorized) _expired.tryEmit(Unit) }
    }

    private companion object { val REFRESH_ON = setOf("order.updated", "debt.updated", "resync") }
}
