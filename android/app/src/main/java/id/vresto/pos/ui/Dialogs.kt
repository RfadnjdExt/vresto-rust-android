package id.vresto.pos.ui

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import id.vresto.pos.core.rupiah
import id.vresto.pos.data.ModifierDto
import id.vresto.pos.data.ProductDto
import id.vresto.pos.data.VariantDto

@Composable
fun MoneyField(value: String, onChange: (String) -> Unit, label: String, modifier: Modifier = Modifier) {
    OutlinedTextField(
        value, { onChange(it.filter(Char::isDigit).take(12)) }, label = { Text(label) }, prefix = { Text("Rp") }, singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = modifier.fillMaxWidth(),
    )
}

@Composable
fun ProductOptionsDialog(p: ProductDto, onDismiss: () -> Unit, onAdd: (VariantDto?, List<Pair<ModifierDto, Int>>) -> Unit) {
    var variant by remember { mutableStateOf(p.variants.firstOrNull()) }
    val picked = remember { mutableStateMapOf<Long, Int>() }
    val missingRequired = p.modifiers.any { it.isRequired && (picked[it.id] ?: 0) == 0 }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(p.name) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                if (p.variants.isNotEmpty()) {
                    Text("Varian", fontWeight = FontWeight.SemiBold)
                    p.variants.forEach { v ->
                        Row(Modifier.fillMaxWidth().clickable { variant = v }, verticalAlignment = Alignment.CenterVertically) {
                            RadioButton(variant?.id == v.id, { variant = v })
                            Text(v.name, Modifier.weight(1f))
                            if (v.priceAdjustment != 0L) Text((if (v.priceAdjustment > 0) "+" else "-") + rupiah(kotlin.math.abs(v.priceAdjustment)))
                        }
                    }
                }
                if (p.modifiers.isNotEmpty()) {
                    Spacer(Modifier.height(8.dp))
                    Text("Tambahan", fontWeight = FontWeight.SemiBold)
                    p.modifiers.forEach { m ->
                        val q = picked[m.id] ?: 0
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(m.name + if (m.isRequired) " *" else "")
                                if (m.price > 0) Text("+" + rupiah(m.price), style = MaterialTheme.typography.labelSmall)
                            }
                            TextButton({ if (q > 0) picked[m.id] = q - 1 }, enabled = q > 0) { Text("−") }
                            Text("$q")
                            TextButton({ if (q < m.maxQuantity) picked[m.id] = q + 1 }, enabled = q < m.maxQuantity) { Text("+") }
                        }
                    }
                }
            }
        },
        confirmButton = {
            Button(
                { onAdd(variant, p.modifiers.mapNotNull { m -> (picked[m.id] ?: 0).takeIf { it > 0 }?.let { m to it } }) },
                enabled = !missingRequired,
            ) { Text("Tambah") }
        },
        dismissButton = { TextButton(onDismiss) { Text("Batal") } },
    )
}

@Composable
fun PaymentDialog(vm: PosViewModel, target: PayTarget) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    val cart by vm.cart.collectAsStateWithLifecycle()
    val methods = ui.paymentMethods.ifEmpty { listOf(id.vresto.pos.data.PaymentMethodDto("cash", "Tunai")) }
    var method by remember { mutableStateOf(methods.first().id) }
    var amount by remember { mutableStateOf("") }
    var customerId by remember { mutableStateOf(cart.customerId) }
    var customerMenu by remember { mutableStateOf(false) }

    val needsCustomer = method == "debt" || method == "points"
    val isCash = method == "cash"
    val paid = if (isCash) amount.toLongOrNull() ?: 0 else target.total
    val change = paid - target.total
    val canPay = !ui.busy && (if (needsCustomer) customerId != null else paid >= target.total)

    AlertDialog(
        onDismissRequest = { if (!ui.busy) vm.dismissPayment() },
        title = { Text("Pembayaran") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                Text(rupiah(target.total), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.horizontalScroll(rememberScrollState())) {
                    methods.forEach { m -> FilterChip(method == m.id, { method = m.id }, label = { Text(m.name) }) }
                }
                if (isCash) {
                    Spacer(Modifier.height(8.dp))
                    MoneyField(amount, { amount = it }, "Uang diterima")
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 4.dp)) {
                        AssistChip({ amount = target.total.toString() }, label = { Text("Pas") })
                        listOf(50_000L, 100_000L).filter { it >= target.total }.forEach { v -> AssistChip({ amount = v.toString() }, label = { Text(rupiah(v)) }) }
                    }
                    Text(if (change >= 0) "Kembalian: ${rupiah(change)}" else "Kurang: ${rupiah(-change)}",
                        color = if (change >= 0) MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.error, fontWeight = FontWeight.SemiBold)
                }
                if (needsCustomer) {
                    Spacer(Modifier.height(8.dp))
                    Box {
                        val c = ui.customers.firstOrNull { it.id == customerId }
                        OutlinedButton({ customerMenu = true }, Modifier.fillMaxWidth()) { Text(c?.name ?: "Pilih pelanggan") }
                        DropdownMenu(customerMenu, { customerMenu = false }) {
                            ui.customers.forEach { cu ->
                                DropdownMenuItem(
                                    text = { Text("${cu.name} · poin ${cu.points} · limit ${rupiah(cu.creditLimit - cu.totalDebt)}") },
                                    onClick = { customerId = cu.id; customerMenu = false },
                                )
                            }
                        }
                    }
                    if (method == "debt") Text("Dicatat sebagai piutang; belum masuk kas.", style = MaterialTheme.typography.labelSmall)
                }
            }
        },
        confirmButton = { Button({ vm.confirmPay(method, if (needsCustomer) 0 else paid, customerId) }, enabled = canPay) { Text(if (ui.busy) "Memproses…" else "Konfirmasi") } },
        dismissButton = { TextButton({ vm.dismissPayment() }, enabled = !ui.busy) { Text("Batal") } },
    )
}

@Composable
fun ReceiptDialog(vm: PosViewModel, view: ReceiptView) {
    AlertDialog(
        onDismissRequest = vm::closeReceipt,
        title = { Text("Pembayaran berhasil") },
        text = {
            Column {
                Text(view.pay.trxNumber, style = MaterialTheme.typography.labelLarge)
                Text("Total ${rupiah(view.pay.totalAmount)}", fontWeight = FontWeight.Bold)
                if (view.pay.changeAmount > 0) Text("Kembalian ${rupiah(view.pay.changeAmount)}", style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.tertiary)
                view.receipt.queueNumber?.let { Text("No. antrean: $it", fontWeight = FontWeight.SemiBold) }
            }
        },
        confirmButton = { Button(vm::closeReceipt) { Text("Selesai") } },
        dismissButton = { OutlinedButton({ vm.print(view.receipt) }) { Text("Cetak struk") } },
    )
}

@Composable
fun SettingsDialog(vm: PosViewModel, onDismiss: () -> Unit, onLogout: () -> Unit) {
    var granted by remember { mutableStateOf(vm.hasBluetoothPermission()) }
    var selected by remember { mutableStateOf(vm.selectedPrinter()) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Pengaturan") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                Text("Printer Bluetooth", fontWeight = FontWeight.SemiBold)
                Text("Pasangkan (pair) printer di pengaturan Bluetooth Android lebih dulu.", style = MaterialTheme.typography.labelSmall)
                if (!granted && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    Button({ launcher.launch(Manifest.permission.BLUETOOTH_CONNECT) }, Modifier.padding(top = 8.dp)) { Text("Izinkan Bluetooth") }
                } else {
                    val devices = remember(granted) { vm.pairedPrinters() }
                    if (devices.isEmpty()) Text("Belum ada perangkat terpasang.", Modifier.padding(top = 8.dp))
                    devices.forEach { d ->
                        Row(Modifier.fillMaxWidth().clickable { selected = d.mac; vm.selectPrinter(d.mac) }, verticalAlignment = Alignment.CenterVertically) {
                            RadioButton(selected == d.mac, { selected = d.mac; vm.selectPrinter(d.mac) })
                            Text(d.name)
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Tutup") } },
        dismissButton = { TextButton(onLogout) { Text("Keluar", color = MaterialTheme.colorScheme.error) } },
    )
}
