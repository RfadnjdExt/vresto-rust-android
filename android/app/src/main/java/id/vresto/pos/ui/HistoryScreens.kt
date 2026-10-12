package id.vresto.pos.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import id.vresto.pos.core.OrderType
import id.vresto.pos.core.TxRules
import id.vresto.pos.core.formatHour
import id.vresto.pos.core.formatLocal
import id.vresto.pos.core.rupiah
import id.vresto.pos.data.DebtDto
import id.vresto.pos.data.PaymentMethodDto
import id.vresto.pos.data.TransactionDto
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.util.UUID

private val DAY = DateTimeFormatter.ofPattern("dd/MM/yyyy")

@Composable
fun HistoryScreen(vm: HistoryViewModel, pos: PosViewModel) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    val posUi by pos.ui.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableIntStateOf(0) }
    LaunchedEffect(Unit) { vm.loadTransactions(); vm.loadDebts() }

    val ctx = remember(posUi.shop, posUi.user, posUi.paymentMethods) {
        SlipContext(posUi.shop, posUi.user?.name, posUi.paymentMethods.associate { it.id to it.name })
    }
    val me = posUi.user
    val myShiftId = posUi.shift?.shift?.id

    Column(Modifier.fillMaxSize()) {
        TabRow(selectedTabIndex = tab) {
            Tab(selected = tab == 0, onClick = { tab = 0 }, text = { Text("Transaksi") })
            Tab(selected = tab == 1, onClick = { tab = 1 }, text = { Text("Hutang") })
        }
        if (tab == 0) TransactionsPane(vm, ui) else DebtsPane(vm, ui, posUi.stats.receivablesTotal)
    }

    ui.detail?.let { d ->
        TransactionDetailDialog(
            d, ui.busy, me?.isAdmin == true, me?.id ?: 0, myShiftId, posUi.refundRequiresAdmin, ctx,
            onDismiss = vm::closeTransaction,
            onVoid = { reason -> vm.voidTransaction(d.trx.id, reason) },
            onRefund = { amount, reason, restock, attempt -> vm.refund(d.trx, amount, reason, restock, attempt, ctx) },
        )
    }
    ui.refundDone?.let { s ->
        AlertDialog(
            onDismissRequest = vm::dismissRefundDone, title = { Text(if (s.isFull) "Refund penuh dicatat" else "Refund sebagian dicatat") },
            text = {
                Column {
                    Text(rupiah(s.amount), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
                    Text(if (s.cashOut) "Ambil ${rupiah(s.amount)} dari laci dan serahkan ke pelanggan." else "Dikembalikan lewat ${s.methodLabel}; kas laci tidak berubah.")
                    Text("Sisa yang masih bisa direfund: ${rupiah(s.remaining)}", style = MaterialTheme.typography.labelMedium)
                }
            },
            confirmButton = { Button(vm::dismissRefundDone) { Text("Selesai") } },
            dismissButton = { OutlinedButton({ vm.printRefund(s) }) { Text("Cetak bukti") } },
        )
    }

    ui.debtDetail?.let { d ->
        DebtDetailDialog(
            d, ui, ui.busy, me?.isAdmin == true, me?.id ?: 0, myShiftId, myShiftId != null,
            posUi.paymentMethods, ctx,
            onDismiss = vm::closeDebt,
            onRepay = { amount, method, notes, whole, attempt -> vm.repay(d, amount, method, notes, whole, attempt, ctx) },
            onReverse = { paymentId, reason -> vm.reversePayment(paymentId, reason) },
        )
    }
    ui.repayDone?.let { s ->
        AlertDialog(
            onDismissRequest = vm::dismissRepayDone, title = { Text(if (s.remainingAfter == 0L) "Hutang lunas" else "Pembayaran dicatat") },
            text = {
                Column {
                    Text(rupiah(s.amount), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
                    Text("${s.customerName} · ${s.methodLabel}")
                    Text("Sisa hutang pelanggan: ${rupiah(s.remainingAfter)}", style = MaterialTheme.typography.labelMedium)
                }
            },
            confirmButton = { Button(vm::dismissRepayDone) { Text("Selesai") } },
            dismissButton = { OutlinedButton({ vm.printDebt(s) }) { Text("Cetak bukti") } },
        )
    }
}

@Composable
private fun StatusChip(text: String, color: Color) {
    Surface(color = color.copy(alpha = 0.15f), contentColor = color, shape = MaterialTheme.shapes.small) {
        Text(text, Modifier.padding(horizontal = 8.dp, vertical = 2.dp), style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.SemiBold)
    }
}

private fun trxColor(status: String) = when (status) {
    "confirmed" -> Color(0xFF2E7D32)
    "partial_refund" -> Color(0xFFEF6C00)
    "refunded", "voided", "cancelled", "failed" -> Color(0xFFC62828)
    "unpaid" -> Color(0xFF5D4037)
    else -> Color(0xFF616161)
}

@Composable
private fun TransactionsPane(vm: HistoryViewModel, ui: HistoryUi) {
    var query by rememberSaveable { mutableStateOf(ui.search) }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton({ vm.shiftDay(-1) }) { Text("‹", style = MaterialTheme.typography.titleLarge) }
            Text(ui.date.format(DAY), Modifier.weight(1f), fontWeight = FontWeight.SemiBold)
            TextButton({ vm.shiftDay(1) }, enabled = ui.date < LocalDate.now()) { Text("›", style = MaterialTheme.typography.titleLarge) }
            TextButton({ vm.setDate(LocalDate.now()) }, enabled = ui.date != LocalDate.now()) { Text("Hari ini") }
        }
        Row(Modifier.padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("all" to "Semua", "completed" to "Berhasil", "cancelled" to "Batal/Refund").forEach { (k, label) ->
                FilterChip(ui.status == k, { vm.setStatus(k) }, label = { Text(label) })
            }
        }
        OutlinedTextField(
            query, { query = it }, placeholder = { Text("Cari no. transaksi / pelanggan") }, singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search), keyboardActions = KeyboardActions(onSearch = { vm.setSearch(query) }),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        )
        ElevatedCard(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp)) {
            Row(Modifier.padding(12.dp)) {
                Column(Modifier.weight(1f)) {
                    Text("Penjualan bersih", style = MaterialTheme.typography.labelSmall)
                    Text(rupiah(ui.summary.totalSales), fontWeight = FontWeight.Bold)
                }
                Column(Modifier.weight(1f)) {
                    Text("Transaksi", style = MaterialTheme.typography.labelSmall)
                    Text("${ui.summary.totalTransactions}", fontWeight = FontWeight.Bold)
                }
                Column(Modifier.weight(1f)) {
                    Text("Batal/refund", style = MaterialTheme.typography.labelSmall)
                    Text("${ui.summary.cancelledCount}", fontWeight = FontWeight.Bold)
                }
            }
        }
        if (ui.loading) LinearProgressIndicator(Modifier.fillMaxWidth())
        if (ui.trx.isEmpty() && !ui.loading) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("Tidak ada transaksi", color = MaterialTheme.colorScheme.outline) }
        } else LazyColumn(contentPadding = PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(ui.trx, key = { it.id }) { t -> TransactionRow(t) { vm.openTransaction(t.id) } }
        }
    }
}

@Composable
private fun TransactionRow(t: TransactionDto, onClick: () -> Unit) {
    ElevatedCard(Modifier.fillMaxWidth().clickable(onClick = onClick)) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(t.trxNumber, fontWeight = FontWeight.SemiBold)
                Text(listOfNotNull(t.orderNumber, t.customerName, formatHour(t.paidAt ?: t.createdAt).ifEmpty { null }).joinToString(" · "), style = MaterialTheme.typography.bodySmall)
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(rupiah(t.totalAmount), fontWeight = FontWeight.Bold,
                    textDecoration = if (t.status == "voided" || t.status == "refunded") TextDecoration.LineThrough else null)
                if (t.refundedAmount > 0 && t.status == "partial_refund") Text("−${rupiah(t.refundedAmount)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFFEF6C00))
                StatusChip(TxRules.statusLabel(t.status), trxColor(t.status))
            }
        }
    }
}

@Composable
private fun TransactionDetailDialog(
    d: TxDetail, busy: Boolean, isAdmin: Boolean, myUserId: Long, myShiftId: Long?, refundRequiresAdmin: Boolean, ctx: SlipContext,
    onDismiss: () -> Unit, onVoid: (String) -> Unit, onRefund: (Long, String, Boolean, String) -> Unit,
) {
    val t = d.trx
    var showVoid by remember { mutableStateOf(false) }
    var showRefund by remember { mutableStateOf(false) }
    val remaining = TxRules.remainingRefundable(t.totalAmount, t.refundedAmount)
    val canVoid = TxRules.canVoid(t.status, t.orderStatus, t.refundedAmount, t.shiftId, t.userId, myShiftId, myUserId, isAdmin)
    val canRefund = TxRules.canRefund(t.status, remaining, isAdmin, refundRequiresAdmin)
    val o = d.order

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(t.trxNumber) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                StatusChip(TxRules.statusLabel(t.status), trxColor(t.status))
                Text(listOfNotNull(t.orderNumber, t.orderType?.let { OrderType.parse(it)?.label }, t.customerName).joinToString(" · "))
                Text("Dibayar ${formatLocal(t.paidAt ?: t.createdAt)} · ${ctx.label(t.paymentMethod)}" + (t.cashier?.let { " · kasir $it" } ?: ""), style = MaterialTheme.typography.bodySmall)
                HorizontalDivider(Modifier.padding(vertical = 4.dp))
                o?.items?.forEach { i ->
                    Row { Text("${i.quantity}× ${i.productName}", Modifier.weight(1f)); Text(rupiah(i.totalPrice)) }
                    i.modifiers.forEach { m -> Text("  + ${m.name}", style = MaterialTheme.typography.labelSmall) }
                }
                o?.let {
                    HorizontalDivider(Modifier.padding(vertical = 4.dp))
                    TotalRow("Subtotal", it.subtotal)
                    if (it.discountAmount > 0) TotalRow("Diskon", -it.discountAmount)
                    if (it.taxAmount > 0) TotalRow("Pajak", it.taxAmount)
                    if (it.serviceCharge > 0) TotalRow("Service", it.serviceCharge)
                    if (it.packagingFee > 0) TotalRow("Kemasan", it.packagingFee)
                }
                TotalRow("TOTAL", t.totalAmount, bold = true)
                if (t.refunds.isNotEmpty()) {
                    HorizontalDivider(Modifier.padding(vertical = 4.dp))
                    Text("Riwayat refund", fontWeight = FontWeight.SemiBold)
                    t.refunds.forEach { r ->
                        TotalRow("${formatLocal(r.createdAt)} · ${r.user.orEmpty()}", -r.amount)
                        Text("“${r.reason}”" + if (r.isRestocked) " · stok dikembalikan" else "", style = MaterialTheme.typography.labelSmall)
                    }
                    TotalRow("Sisa bisa direfund", remaining)
                }
                t.void?.let { v ->
                    HorizontalDivider(Modifier.padding(vertical = 4.dp))
                    Text("Di-void oleh ${v.user.orEmpty()} (${formatLocal(v.createdAt)})", fontWeight = FontWeight.SemiBold)
                    Text("“${v.reason}”", style = MaterialTheme.typography.bodySmall)
                }
                if (canVoid || canRefund) {
                    Spacer(Modifier.height(8.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (canVoid) OutlinedButton({ showVoid = true }, enabled = !busy, colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.error)) { Text("Void") }
                        if (canRefund) Button({ showRefund = true }, enabled = !busy) { Text("Refund") }
                    }
                    if (canVoid) Text("Void: untuk transaksi yang salah, sebelum pesanan selesai.", style = MaterialTheme.typography.labelSmall)
                } else if (t.status == "confirmed" && !isAdmin && refundRequiresAdmin) {
                    Text("Refund hanya bisa diproses admin.", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.outline)
                }
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Tutup") } },
    )

    if (showVoid) ReasonDialog("Void ${t.trxNumber}?", "Alasan void", "Void", destructive = true, busy = busy,
        note = "Transaksi dibatalkan seolah tak pernah terjadi. Pesanan dibatalkan dan meja dibebaskan.",
        onConfirm = { onVoid(it); showVoid = false }, onDismiss = { showVoid = false })
    if (showRefund) RefundDialog(t, d.order?.status ?: t.orderStatus, busy, ctx,
        onConfirm = { amount, reason, restock, attempt -> onRefund(amount, reason, restock, attempt); showRefund = false }, onDismiss = { showRefund = false })
}

@Composable
fun ReasonDialog(
    title: String, label: String, confirmText: String, busy: Boolean, onConfirm: (String) -> Unit, onDismiss: () -> Unit,
    destructive: Boolean = false, note: String? = null,
) {
    var reason by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss, title = { Text(title) },
        text = {
            Column {
                note?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                OutlinedTextField(reason, { reason = it }, label = { Text("$label (wajib)") }, modifier = Modifier.fillMaxWidth().padding(top = 8.dp))
            }
        },
        confirmButton = {
            Button({ onConfirm(reason.trim()) }, enabled = reason.isNotBlank() && !busy,
                colors = if (destructive) ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error) else ButtonDefaults.buttonColors()) { Text(confirmText) }
        },
        dismissButton = { TextButton(onDismiss) { Text("Batal") } },
    )
}

@Composable
private fun RefundDialog(
    t: TransactionDto, orderStatus: String?, busy: Boolean, ctx: SlipContext,
    onConfirm: (Long, String, Boolean, String) -> Unit, onDismiss: () -> Unit,
) {
    val remaining = TxRules.remainingRefundable(t.totalAmount, t.refundedAmount)
    val fixed = TxRules.mustRefundFully(t.paymentMethod)

    val attemptId = remember { UUID.randomUUID().toString() }
    var amountText by remember { mutableStateOf(remaining.toString()) }
    var reason by remember { mutableStateOf("") }
    var restock by remember { mutableStateOf(false) }
    val amount = amountText.toLongOrNull() ?: 0
    val error = TxRules.validateRefundAmount(amount, remaining)
    val restockAllowed = TxRules.canRestock(amount, remaining, orderStatus)
    val effectiveRestock = restock && restockAllowed

    AlertDialog(
        onDismissRequest = onDismiss, title = { Text("Refund ${t.trxNumber}") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Total ${rupiah(t.totalAmount)} · sisa bisa direfund ${rupiah(remaining)}", style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(
                    amountText, { amountText = it.filter(Char::isDigit).take(12) }, label = { Text("Jumlah refund") }, prefix = { Text("Rp") },
                    singleLine = true, enabled = !fixed, isError = error != null && amountText.isNotEmpty(),
                    supportingText = { (if (fixed) "Transaksi poin hanya bisa direfund penuh." else error)?.let { Text(it) } },
                    keyboardOptions = KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Number), modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(reason, { reason = it }, label = { Text("Alasan (wajib)") }, modifier = Modifier.fillMaxWidth())
                if (restockAllowed) Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) { Text("Kembalikan stok"); Text("Barang kembali ke persediaan", style = MaterialTheme.typography.labelSmall) }
                    Switch(restock, { restock = it })
                }
                Text(
                    if (t.paymentMethod == "cash") "Uang dikembalikan TUNAI — diambil dari laci shift Anda." else "Dikembalikan lewat ${ctx.label(t.paymentMethod)}; kas laci tidak berubah.",
                    style = MaterialTheme.typography.labelSmall,
                )
            }
        },
        confirmButton = { Button({ onConfirm(amount, reason.trim(), effectiveRestock, attemptId) }, enabled = error == null && reason.isNotBlank() && !busy) { Text("Proses refund") } },
        dismissButton = { TextButton(onDismiss) { Text("Batal") } },
    )
}

@Composable
private fun DebtsPane(vm: HistoryViewModel, ui: HistoryUi, receivablesTotal: Long) {
    Column(Modifier.fillMaxSize()) {
        ElevatedCard(Modifier.fillMaxWidth().padding(12.dp)) {
            Column(Modifier.padding(12.dp)) {
                Text("Total piutang berjalan", style = MaterialTheme.typography.labelSmall)
                Text(rupiah(receivablesTotal), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            }
        }
        Row(Modifier.padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("open" to "Aktif", "paid" to "Lunas", "all" to "Semua").forEach { (k, label) ->
                FilterChip(ui.debtFilter == k, { vm.setDebtFilter(k) }, label = { Text(label) })
            }
        }
        if (ui.debts.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("Tidak ada hutang", color = MaterialTheme.colorScheme.outline) }
        } else LazyColumn(contentPadding = PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(ui.debts, key = { it.id }) { d ->
                ElevatedCard(Modifier.fillMaxWidth().clickable { vm.openDebt(d.id) }) {
                    Column(Modifier.padding(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(d.customerName, Modifier.weight(1f), fontWeight = FontWeight.SemiBold)
                            if (d.isOverdue) { StatusChip("Jatuh tempo", Color(0xFFC62828)); Spacer(Modifier.width(6.dp)) }
                            StatusChip(TxRules.debtStatusLabel(d.status), debtColor(d.status))
                        }
                        Text(listOfNotNull(d.orderNumber, d.dueDate?.let { "tempo $it" }).joinToString(" · "), style = MaterialTheme.typography.bodySmall)
                        Row {
                            Text("Sisa ${rupiah(d.remainingAmount)}", Modifier.weight(1f), fontWeight = FontWeight.Bold)
                            Text("dari ${rupiah(d.amount)}", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        }
    }
}

private fun debtColor(status: String) = when (status) {
    "PAID" -> Color(0xFF2E7D32)
    "PARTIAL" -> Color(0xFFEF6C00)
    "VOID" -> Color(0xFF616161)
    else -> Color(0xFF5D4037)
}

@Composable
private fun DebtDetailDialog(
    d: DebtDto, ui: HistoryUi, busy: Boolean, isAdmin: Boolean, myUserId: Long, myShiftId: Long?, hasShift: Boolean,
    methods: List<PaymentMethodDto>, ctx: SlipContext,
    onDismiss: () -> Unit,
    onRepay: (Long, String, String?, Boolean, String) -> Unit,
    onReverse: (Long, String) -> Unit,
) {
    var showRepay by remember { mutableStateOf(false) }
    var reversing by remember { mutableStateOf<Long?>(null) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(d.customerName) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    StatusChip(TxRules.debtStatusLabel(d.status), debtColor(d.status))
                    if (d.isOverdue) { Spacer(Modifier.width(6.dp)); StatusChip("Jatuh tempo", Color(0xFFC62828)) }
                }
                Text(listOfNotNull(d.orderNumber, d.dueDate?.let { "tempo $it" }).joinToString(" · "))
                TotalRow("Nilai hutang", d.amount)
                TotalRow("Sisa", d.remainingAmount, bold = true)
                if (ui.customerOpenDebts.size > 1) Text("Pelanggan ini punya ${ui.customerOpenDebts.size} hutang aktif (total ${rupiah(ui.customerOpenTotal)}).", style = MaterialTheme.typography.labelSmall)
                if (d.payments.isNotEmpty()) {
                    HorizontalDivider(Modifier.padding(vertical = 4.dp))
                    Text("Riwayat pembayaran", fontWeight = FontWeight.SemiBold)
                    d.payments.forEach { p ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text("${formatLocal(p.createdAt)} · ${ctx.label(p.paymentMethod)}", style = MaterialTheme.typography.bodySmall)
                                if (p.isReversed) Text("Dibatalkan: ${p.reversalReason.orEmpty()}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.error)
                            }
                            Text(rupiah(p.amount), textDecoration = if (p.isReversed) TextDecoration.LineThrough else null)
                            if (TxRules.canReversePayment(p.isReversed, isAdmin, p.userId, p.shiftId, myUserId, myShiftId)) {
                                TextButton({ reversing = p.id }, enabled = !busy) { Text("Batalkan", color = MaterialTheme.colorScheme.error) }
                            }
                        }
                    }
                }
                if (TxRules.canRepay(d.status, d.remainingAmount)) {
                    Spacer(Modifier.height(8.dp))
                    Button({ showRepay = true }, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text("Terima pembayaran") }
                    if (!hasShift) Text("Buka shift dulu untuk menerima pembayaran.", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.error)
                }
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Tutup") } },
    )
    if (showRepay) RepayDialog(d, ui.customerOpenTotal, methods, busy,
        onConfirm = { amount, method, notes, whole, attempt -> onRepay(amount, method, notes, whole, attempt); showRepay = false }, onDismiss = { showRepay = false })
    reversing?.let { pid ->
        ReasonDialog("Batalkan pembayaran ini?", "Alasan pembatalan", "Batalkan pembayaran", destructive = true, busy = busy,
            note = "Sisa hutang dipulihkan. Bila pembayaran tunai, uang dikembalikan dari laci.",
            onConfirm = { onReverse(pid, it); reversing = null }, onDismiss = { reversing = null })
    }
}

@Composable
private fun RepayDialog(
    d: DebtDto, customerOpenTotal: Long, methods: List<PaymentMethodDto>, busy: Boolean,
    onConfirm: (Long, String, String?, Boolean, String) -> Unit, onDismiss: () -> Unit,
) {
    val usable = methods.filter { it.id in TxRules.repayMethods(methods.map { m -> m.id }) }.ifEmpty { listOf(PaymentMethodDto("cash", "Tunai")) }
    val attemptId = remember { UUID.randomUUID().toString() }
    val canWhole = customerOpenTotal > d.remainingAmount
    var whole by remember { mutableStateOf(false) }
    val max = if (whole) customerOpenTotal else d.remainingAmount
    var amountText by remember(max) { mutableStateOf(max.toString()) }
    var method by remember { mutableStateOf(usable.first().id) }
    var notes by remember { mutableStateOf("") }
    val amount = amountText.toLongOrNull() ?: 0
    val error = TxRules.validateRepayAmount(amount, max)

    AlertDialog(
        onDismissRequest = onDismiss, title = { Text("Terima pembayaran") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (canWhole) Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("Bayar semua hutang ${d.customerName}")
                        Text("Dialokasikan ke yang jatuh tempo lebih dulu", style = MaterialTheme.typography.labelSmall)
                    }
                    Switch(whole, { whole = it })
                }
                Text("Maksimal ${rupiah(max)}", style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(
                    amountText, { amountText = it.filter(Char::isDigit).take(12) }, label = { Text("Jumlah dibayar") }, prefix = { Text("Rp") }, singleLine = true,
                    isError = error != null && amountText.isNotEmpty(), supportingText = { error?.let { Text(it) } },
                    keyboardOptions = KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Number), modifier = Modifier.fillMaxWidth(),
                )
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    usable.forEach { m -> FilterChip(method == m.id, { method = m.id }, label = { Text(m.name) }) }
                }
                OutlinedTextField(notes, { notes = it }, label = { Text("Catatan (opsional)") }, modifier = Modifier.fillMaxWidth())
                Text(if (method == "cash") "Tunai masuk ke laci shift Anda." else "Non-tunai: dicatat, tidak menambah kas laci.", style = MaterialTheme.typography.labelSmall)
            }
        },
        confirmButton = { Button({ onConfirm(amount, method, notes.ifBlank { null }, whole && canWhole, attemptId) }, enabled = error == null && !busy) { Text("Terima") } },
        dismissButton = { TextButton(onDismiss) { Text("Batal") } },
    )
}
