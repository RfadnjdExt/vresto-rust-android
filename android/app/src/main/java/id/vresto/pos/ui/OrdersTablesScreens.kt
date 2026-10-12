package id.vresto.pos.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.clickable
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import id.vresto.pos.core.OrderType
import id.vresto.pos.core.rupiah
import id.vresto.pos.data.OrderDto

private fun statusLabel(s: String) = when (s) {
    "pending" -> "Menunggu"; "confirmed" -> "Dikonfirmasi"; "preparing" -> "Diproses"
    "ready" -> "Siap"; "served" -> "Disajikan"; "completed" -> "Selesai"; "cancelled" -> "Batal"; else -> s
}

@Composable
fun OrdersScreen(vm: PosViewModel) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    var cancelTarget by remember { mutableStateOf<OrderDto?>(null) }

    if (ui.orders.isEmpty()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("Tidak ada pesanan aktif", color = MaterialTheme.colorScheme.outline) }
    } else LazyColumn(contentPadding = PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(ui.orders, key = { it.id }) { o ->
            ElevatedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(o.orderNumber, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
                        AssistChip({}, label = { Text(statusLabel(o.status)) })
                    }
                    Text(listOfNotNull(OrderType.parse(o.orderType)?.label, o.tableName?.let { "Meja $it" },
                        o.queueNumber?.let { "Antrean $it" }, o.customerDisplay).joinToString(" · "), style = MaterialTheme.typography.bodySmall)
                    Spacer(Modifier.height(4.dp))
                    o.items.forEach { i -> Text("${i.quantity}× ${i.productName}", style = MaterialTheme.typography.bodyMedium) }
                    Spacer(Modifier.height(4.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(rupiah(o.totalAmount), fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
                        Text(if (o.isPaid) "Lunas" else "Belum dibayar", color = if (o.isPaid) MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.error)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                        if (!o.isPaid) {
                            Button({ vm.openPaymentForOrder(o) }) { Text("Bayar") }
                            OutlinedButton({ cancelTarget = o }) { Text("Batal") }
                        } else {
                            if (o.status == "preparing") OutlinedButton({ vm.setStatus(o.id, "ready") }) { Text("Siap") }
                            if (o.status == "ready") OutlinedButton({ vm.setStatus(o.id, "served") }) { Text("Disajikan") }
                            Button({ vm.completeOrder(o.id) }, enabled = !ui.busy) { Text("Selesai") }
                        }
                    }
                }
            }
        }
    }

    cancelTarget?.let { o ->
        AlertDialog(
            onDismissRequest = { cancelTarget = null },
            title = { Text("Batalkan ${o.orderNumber}?") },
            text = { Text("Meja akan dibebaskan. Tindakan ini tidak bisa dibatalkan.") },
            confirmButton = { Button({ vm.cancelOrder(o.id); cancelTarget = null }) { Text("Ya, batalkan") } },
            dismissButton = { TextButton({ cancelTarget = null }) { Text("Tidak") } },
        )
    }
}

@Composable
fun TablesScreen(vm: PosViewModel, onPickTable: (Long) -> Unit) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    var busyInfo by remember { mutableStateOf<String?>(null) }
    LazyVerticalGrid(GridCells.Adaptive(110.dp), contentPadding = PaddingValues(12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(ui.tables, key = { it.id }) { t ->
            val (bg, fg) = when (t.status) {
                "occupied" -> Color(0xFFFFCDD2) to Color(0xFFB71C1C)
                "reserved" -> Color(0xFFFFF9C4) to Color(0xFFF57F17)
                else -> Color(0xFFC8E6C9) to Color(0xFF1B5E20)
            }
            Card(
                colors = CardDefaults.cardColors(containerColor = bg, contentColor = fg),
                modifier = Modifier.fillMaxWidth().clickable {
                    if (t.status == "available") onPickTable(t.id) else busyInfo = "Meja ${t.name} ${if (t.status == "occupied") "terisi" else "dipesan"}" + (t.currentOrderNumber?.let { " ($it)" } ?: "")
                },
            ) {
                Column(Modifier.padding(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(t.name, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                    Text("${t.capacity} kursi", style = MaterialTheme.typography.labelSmall)
                    Text(when (t.status) { "occupied" -> "Terisi"; "reserved" -> "Dipesan"; else -> "Kosong" }, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
    busyInfo?.let { AlertDialog({ busyInfo = null }, confirmButton = { TextButton({ busyInfo = null }) { Text("OK") } }, text = { Text(it) }) }
}
