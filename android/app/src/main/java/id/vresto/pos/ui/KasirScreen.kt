package id.vresto.pos.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material.icons.filled.ShoppingCart
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import id.vresto.pos.core.Discount
import id.vresto.pos.core.OrderType
import id.vresto.pos.core.rupiah
import id.vresto.pos.data.ProductDto

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun KasirScreen(vm: PosViewModel) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    val cart by vm.cart.collectAsStateWithLifecycle()
    var query by rememberSaveable { mutableStateOf("") }
    var category by rememberSaveable { mutableStateOf<Long?>(null) }
    var options by remember { mutableStateOf<ProductDto?>(null) }
    var showCart by rememberSaveable { mutableStateOf(false) }

    val products = remember(ui.products, query, category) {
        ui.products.filter { p ->
            (category == null || p.categoryId == category) && (query.isBlank() || p.name.contains(query, ignoreCase = true))
        }
    }
    val catalog: @Composable (Modifier) -> Unit = { mod ->
        Column(mod) {
            OutlinedTextField(query, { query = it }, placeholder = { Text("Cari menu…") }, singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp))
            LazyRow(contentPadding = PaddingValues(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                item { FilterChip(category == null, { category = null }, label = { Text("Semua") }) }
                items(ui.categories, key = { it.id }) { c -> FilterChip(category == c.id, { category = c.id }, label = { Text(c.name) }) }
            }
            LazyVerticalGrid(
                GridCells.Adaptive(150.dp), contentPadding = PaddingValues(12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.weight(1f),
            ) {
                items(products, key = { it.id }) { p ->
                    ProductCard(p) { if (p.hasOptions) options = p else vm.addToCart(p) }
                }
            }
        }
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        if (maxWidth >= 840.dp) {

            Row(Modifier.fillMaxSize()) {
                catalog(Modifier.weight(1.4f).fillMaxHeight())
                VerticalDivider()
                CartPanel(vm, Modifier.weight(1f).fillMaxHeight())
            }
        } else {

            Column(Modifier.fillMaxSize()) {
                catalog(Modifier.weight(1f))
                if (cart.count > 0) {
                    Surface(tonalElevation = 3.dp, modifier = Modifier.fillMaxWidth().clickable { showCart = true }) {
                        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Default.ShoppingCart, null)
                            Text("  ${cart.count} item", Modifier.weight(1f), fontWeight = FontWeight.SemiBold)
                            Text(rupiah(vm.totalsFor(cart, ui.billing).total), fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
            if (showCart) ModalBottomSheet({ showCart = false }) { CartPanel(vm, Modifier.fillMaxWidth().heightIn(max = 620.dp)) }
        }
    }
    options?.let { p -> ProductOptionsDialog(p, onDismiss = { options = null }) { v, m -> vm.addToCart(p, v, m); options = null } }
}

@Composable
private fun ProductCard(p: ProductDto, onClick: () -> Unit) {
    val outOfStock = p.isStockManaged && p.stockQuantity <= 0
    ElevatedCard(Modifier.fillMaxWidth().clickable(enabled = !outOfStock, onClick = onClick)) {
        Column(Modifier.padding(12.dp)) {
            Text(p.name, fontWeight = FontWeight.SemiBold, maxLines = 2, minLines = 2)
            Spacer(Modifier.height(4.dp))
            Text(rupiah(p.effectivePrice), color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
            if (p.effectivePrice < p.price) Text(rupiah(p.price), style = MaterialTheme.typography.labelSmall,
                textDecoration = androidx.compose.ui.text.style.TextDecoration.LineThrough)
            when {
                outOfStock -> Text("Habis", color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium)
                p.isLowStock -> Text("Sisa ${p.stockQuantity}", color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium)
                p.hasOptions -> Text("Pilihan ›", style = MaterialTheme.typography.labelMedium)
                else -> Spacer(Modifier.height(16.dp))
            }
        }
    }
}

@Composable
fun CartPanel(vm: PosViewModel, modifier: Modifier = Modifier) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    val cart by vm.cart.collectAsStateWithLifecycle()
    val totals = vm.totalsFor(cart, ui.billing)
    var discountText by rememberSaveable { mutableStateOf("") }
    var discountPercent by rememberSaveable { mutableStateOf(true) }
    var tableMenu by remember { mutableStateOf(false) }

    fun applyDiscount(text: String, percent: Boolean) {
        val n = text.replace(',', '.').toDoubleOrNull()
        vm.setDiscount(when { n == null || n <= 0 -> Discount.None; percent -> Discount.Percent(n.coerceAtMost(100.0)); else -> Discount.Fixed(n.toLong()) })
    }

    Column(modifier.padding(12.dp)) {
        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(listOf(OrderType.DINE_IN, OrderType.TAKE_AWAY, OrderType.DELIVERY)) { t ->
                FilterChip(cart.orderType == t, { vm.setOrderType(t) }, label = { Text(t.label) })
            }
        }
        if (cart.orderType == OrderType.DINE_IN) {
            Box {
                val table = ui.tables.firstOrNull { it.id == cart.tableId }
                OutlinedButton({ tableMenu = true }, Modifier.fillMaxWidth()) { Text(table?.let { "Meja ${it.name}" } ?: "Pilih meja") }
                DropdownMenu(tableMenu, { tableMenu = false }) {
                    DropdownMenuItem(text = { Text("Tanpa meja") }, onClick = { vm.setTable(null); tableMenu = false })
                    ui.tables.filter { it.status == "available" }.forEach { t ->
                        DropdownMenuItem(text = { Text("${t.name} · ${t.capacity} kursi") }, onClick = { vm.setTable(t.id); tableMenu = false })
                    }
                }
            }
        }
        HorizontalDivider(Modifier.padding(vertical = 8.dp))

        if (cart.lines.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) { Text("Keranjang kosong", color = MaterialTheme.colorScheme.outline) }
        } else LazyColumn(Modifier.weight(1f)) {
            items(cart.lines, key = { it.key }) { l ->
                Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(l.title, fontWeight = FontWeight.Medium)
                        l.extras.forEach { Text(it, style = MaterialTheme.typography.labelSmall) }
                        Text(rupiah(l.lineTotal), style = MaterialTheme.typography.bodySmall)
                    }
                    IconButton({ vm.changeQty(l.key, -1) }) { Icon(Icons.Default.Remove, "Kurangi") }
                    Text("${l.qty}", fontWeight = FontWeight.Bold)
                    IconButton({ vm.changeQty(l.key, 1) }) { Icon(Icons.Default.Add, "Tambah") }
                    IconButton({ vm.removeLine(l.key) }) { Icon(Icons.Default.Delete, "Hapus") }
                }
            }
        }

        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(discountText, { discountText = it; applyDiscount(it, discountPercent) }, label = { Text("Diskon") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal), modifier = Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            FilterChip(discountPercent, { discountPercent = true; applyDiscount(discountText, true) }, label = { Text("%") })
            Spacer(Modifier.width(4.dp))
            FilterChip(!discountPercent, { discountPercent = false; applyDiscount(discountText, false) }, label = { Text("Rp") })
        }
        Spacer(Modifier.height(8.dp))
        TotalRow("Subtotal", totals.subtotal)
        if (totals.discountAmount > 0) TotalRow("Diskon", -totals.discountAmount)
        if (totals.taxAmount > 0) TotalRow("Pajak", totals.taxAmount)
        if (totals.serviceCharge > 0) TotalRow("Service", totals.serviceCharge)
        if (totals.packagingFee > 0) TotalRow("Kemasan", totals.packagingFee)
        TotalRow("TOTAL", totals.total, bold = true)
        Text("Total final dihitung server saat pembayaran.", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.outline)
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton({ vm.saveOrder() }, Modifier.weight(1f), enabled = cart.lines.isNotEmpty() && !ui.busy) { Text("Simpan") }
            Button({ vm.openPaymentForCart() }, Modifier.weight(1f), enabled = cart.lines.isNotEmpty() && !ui.busy) { Text("Bayar") }
        }
        if (cart.lines.isNotEmpty()) TextButton({ vm.clearCart(); discountText = ""; vm.setDiscount(Discount.None) }) { Text("Kosongkan") }
    }
}

@Composable
fun TotalRow(label: String, amount: Long, bold: Boolean = false) {
    Row(Modifier.fillMaxWidth()) {
        Text(label, Modifier.weight(1f), fontWeight = if (bold) FontWeight.Bold else null)
        Text(if (amount < 0) "-" + rupiah(-amount) else rupiah(amount), fontWeight = if (bold) FontWeight.Bold else null)
    }
}
