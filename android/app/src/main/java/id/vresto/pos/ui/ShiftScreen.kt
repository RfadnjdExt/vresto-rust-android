package id.vresto.pos.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import id.vresto.pos.core.rupiah
import id.vresto.pos.data.ShiftSummaryDto

private enum class ShiftDialog { ADD_CASH, DROP_CASH, EXPENSE, CLOSE }

@Composable
fun ShiftScreen(vm: PosViewModel) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    val shift = ui.shift
    var dialog by remember { mutableStateOf<ShiftDialog?>(null) }
    var closed by remember { mutableStateOf<ShiftSummaryDto?>(null) }
    LaunchedEffect(shift?.shift?.id) { if (shift != null) vm.loadExpenses() }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (shift == null) {
            var cash by rememberSaveable { mutableStateOf("") }
            Text("Belum ada shift aktif", style = MaterialTheme.typography.titleLarge)
            Text("Buka shift dan hitung uang di laci sebelum menerima pembayaran.")
            MoneyField(cash, { cash = it }, "Kas awal (uang di laci)")
            Button({ vm.startShift(cash.toLongOrNull() ?: 0) }, enabled = cash.isNotEmpty() && !ui.busy, modifier = Modifier.fillMaxWidth()) { Text("Buka Shift") }
        } else {
            ElevatedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("Shift ${shift.shift.shiftNumber}", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                    TotalRow("Kas awal", shift.shift.openingCash)
                    TotalRow("Penjualan tunai", shift.sales.cashSales)
                    TotalRow("Penjualan non-tunai", shift.sales.digitalSales)
                    if (shift.debtCollected.total != 0L) {
                        TotalRow("Pelunasan hutang tunai", shift.debtCollected.cash)
                        if (shift.debtCollected.digital != 0L) TotalRow("Pelunasan hutang non-tunai", shift.debtCollected.digital)
                    }
                    TotalRow("Total penjualan", shift.sales.totalSales)
                    TotalRow("Pengeluaran", -shift.totalExpenses)
                    HorizontalDivider(Modifier.padding(vertical = 4.dp))
                    TotalRow("Kas seharusnya di laci", shift.expectedCash, bold = true)
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton({ dialog = ShiftDialog.ADD_CASH }, Modifier.weight(1f)) { Text("Tambah kas") }
                OutlinedButton({ dialog = ShiftDialog.DROP_CASH }, Modifier.weight(1f)) { Text("Ambil kas") }
            }
            OutlinedButton({ dialog = ShiftDialog.EXPENSE }, Modifier.fillMaxWidth()) { Text("Catat pengeluaran") }
            if (ui.expenses.isNotEmpty()) {
                Text("Pengeluaran shift ini", fontWeight = FontWeight.SemiBold)
                ui.expenses.forEach { e -> TotalRow(e.category + (e.description?.let { " — $it" } ?: ""), e.amount) }
            }
            Button({ dialog = ShiftDialog.CLOSE }, Modifier.fillMaxWidth(), colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)) { Text("Tutup Shift") }
        }
    }

    when (dialog) {
        ShiftDialog.ADD_CASH, ShiftDialog.DROP_CASH -> {
            val add = dialog == ShiftDialog.ADD_CASH
            FormDialog(if (add) "Tambah kas" else "Ambil kas", onDismiss = { dialog = null },
                confirmEnabled = { it.amount.toLongOrNull()?.let { n -> n > 0 } == true && (add || it.note.isNotBlank()) },
                noteLabel = if (add) "Keterangan (opsional)" else "Alasan (wajib)") { f ->
                vm.cashMove(if (add) "add_cash" else "drop_cash", f.amount.toLong(), f.note.ifBlank { null }); dialog = null
            }
        }
        ShiftDialog.EXPENSE -> FormDialog("Catat pengeluaran", onDismiss = { dialog = null }, categoryLabel = "Kategori (mis. Gas, Es batu)",
            confirmEnabled = { it.category.isNotBlank() && (it.amount.toLongOrNull() ?: 0) > 0 }, noteLabel = "Keterangan (opsional)") { f ->
            vm.addExpense(f.category, f.note.ifBlank { null }, f.amount.toLong()); dialog = null
        }
        ShiftDialog.CLOSE -> if (shift != null) CloseShiftDialog(shift, ui.busy, onDismiss = { dialog = null }) { actual, notes ->
            vm.closeShift(actual, notes) { closed = it; dialog = null }
        }
        null -> {}
    }
    closed?.let { s ->
        AlertDialog(onDismissRequest = { closed = null }, title = { Text("Shift ditutup") },
            text = {
                val d = s.shift.cashDifference ?: 0
                Text(if (d == 0L) "Kas sesuai. Tidak ada selisih." else (if (d < 0) "Kurang ${rupiah(-d)}" else "Lebih ${rupiah(d)}") + " dari seharusnya.")
            },
            confirmButton = { TextButton({ closed = null }) { Text("OK") } })
    }
}

private data class Form(val amount: String = "", val note: String = "", val category: String = "")

@Composable
private fun FormDialog(
    title: String, onDismiss: () -> Unit, confirmEnabled: (Form) -> Boolean, noteLabel: String,
    categoryLabel: String? = null, onConfirm: (Form) -> Unit,
) {
    var f by remember { mutableStateOf(Form()) }
    AlertDialog(
        onDismissRequest = onDismiss, title = { Text(title) },
        text = {
            Column {
                if (categoryLabel != null) OutlinedTextField(f.category, { f = f.copy(category = it) }, label = { Text(categoryLabel) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                MoneyField(f.amount, { f = f.copy(amount = it) }, "Nominal")
                OutlinedTextField(f.note, { f = f.copy(note = it) }, label = { Text(noteLabel) }, modifier = Modifier.fillMaxWidth())
            }
        },
        confirmButton = { Button({ onConfirm(f) }, enabled = confirmEnabled(f)) { Text("Simpan") } },
        dismissButton = { TextButton(onDismiss) { Text("Batal") } },
    )
}

@Composable
private fun CloseShiftDialog(s: ShiftSummaryDto, busy: Boolean, onDismiss: () -> Unit, onConfirm: (Long, String?) -> Unit) {
    var actual by remember { mutableStateOf("") }
    var notes by remember { mutableStateOf("") }
    val diff = (actual.toLongOrNull() ?: 0) - s.expectedCash

    val ok = actual.isNotEmpty() && (diff == 0L || notes.isNotBlank()) && !busy
    AlertDialog(
        onDismissRequest = onDismiss, title = { Text("Tutup shift") },
        text = {
            Column {
                Text("Kas seharusnya: ${rupiah(s.expectedCash)}", fontWeight = FontWeight.SemiBold)
                MoneyField(actual, { actual = it }, "Uang di laci (hitung manual)")
                if (actual.isNotEmpty()) Text(
                    if (diff == 0L) "Pas." else if (diff < 0) "Kurang ${rupiah(-diff)}" else "Lebih ${rupiah(diff)}",
                    color = if (diff == 0L) MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.error,
                )
                OutlinedTextField(notes, { notes = it }, label = { Text(if (diff != 0L && actual.isNotEmpty()) "Alasan selisih (wajib)" else "Catatan (opsional)") }, modifier = Modifier.fillMaxWidth())
            }
        },
        confirmButton = { Button({ onConfirm(actual.toLong(), notes.ifBlank { null }) }, enabled = ok) { Text("Tutup shift") } },
        dismissButton = { TextButton(onDismiss) { Text("Batal") } },
    )
}
