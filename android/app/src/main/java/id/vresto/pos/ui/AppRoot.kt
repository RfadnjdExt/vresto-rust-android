package id.vresto.pos.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Payments
import androidx.compose.material.icons.filled.Receipt
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.ShoppingCart
import androidx.compose.material.icons.filled.TableRestaurant
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController

@Composable
fun AppRoot() {
    val nav = rememberNavController()
    val auth: AuthViewModel = hiltViewModel()
    NavHost(nav, startDestination = if (auth.hasSession) "main" else "login") {
        composable("login") {
            LoginScreen(auth) { nav.navigate("main") { popUpTo("login") { inclusive = true } } }
        }
        composable("main") {
            MainScaffold(onLoggedOut = { nav.navigate("login") { popUpTo("main") { inclusive = true } } })
        }
    }
}

@Composable
fun LoginScreen(vm: AuthViewModel, onSuccess: () -> Unit) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    var server by rememberSaveable { mutableStateOf(vm.savedServer.ifEmpty { "192.168.1.10:8000" }) }
    var user by rememberSaveable { mutableStateOf("") }
    var pass by rememberSaveable { mutableStateOf("") }
    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.fillMaxSize().padding(24.dp).widthIn(max = 420.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("Vresto POS", style = MaterialTheme.typography.headlineLarge, color = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.height(24.dp))
            OutlinedTextField(server, { server = it }, label = { Text("Alamat server") }, singleLine = true, modifier = Modifier.fillMaxWidth(),
                supportingText = { Text("Contoh: 192.168.1.10:8000 atau https://pos.tokoanda.id") })
            OutlinedTextField(user, { user = it }, label = { Text("Username") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(pass, { pass = it }, label = { Text("Password") }, singleLine = true, modifier = Modifier.fillMaxWidth(),
                visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password))
            ui.error?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(top = 8.dp)) }
            Spacer(Modifier.height(16.dp))
            Button({ vm.login(server, user, pass, onSuccess) }, enabled = !ui.busy, modifier = Modifier.fillMaxWidth()) {
                if (ui.busy) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp) else Text("Masuk")
            }
        }
    }
}

private data class Tab(val label: String, val icon: ImageVector)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScaffold(onLoggedOut: () -> Unit, vm: PosViewModel = hiltViewModel()) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    val pay by vm.pay.collectAsStateWithLifecycle()
    val receipt by vm.receipt.collectAsStateWithLifecycle()
    val snack = remember { SnackbarHostState() }
    var tab by rememberSaveable { mutableIntStateOf(0) }
    var showSettings by rememberSaveable { mutableStateOf(false) }
    val tabs = listOf(Tab("Kasir", Icons.Default.ShoppingCart), Tab("Pesanan", Icons.Default.Receipt),
        Tab("Meja", Icons.Default.TableRestaurant), Tab("Riwayat", Icons.Default.History), Tab("Shift", Icons.Default.Payments))
    val history: HistoryViewModel = hiltViewModel()

    LaunchedEffect(Unit) { vm.messages.collect { snack.showSnackbar(it) } }
    LaunchedEffect(Unit) { history.messages.collect { snack.showSnackbar(it) } }
    LaunchedEffect(Unit) { history.sessionExpired.collect { vm.logout(); onLoggedOut() } }
    LaunchedEffect(ui.sessionExpired) { if (ui.sessionExpired) { vm.logout(); onLoggedOut() } }

    Scaffold(
        snackbarHost = { SnackbarHost(snack) },
        topBar = {
            TopAppBar(
                title = { Text(ui.shop.name) },
                actions = {

                    Box(Modifier.size(10.dp).background(if (ui.connected) Color(0xFF2E7D32) else Color(0xFFC62828), CircleShape))
                    Spacer(Modifier.width(8.dp))
                    Text(ui.user?.name.orEmpty(), style = MaterialTheme.typography.labelLarge)
                    IconButton({ showSettings = true }) { Icon(Icons.Default.Settings, "Pengaturan") }
                },
            )
        },
        bottomBar = {
            NavigationBar {
                tabs.forEachIndexed { i, t ->
                    NavigationBarItem(
                        selected = tab == i, onClick = { tab = i }, icon = { Icon(t.icon, t.label) }, label = { Text(t.label) },
                    )
                }
            }
        },
    ) { pad ->
        Box(Modifier.padding(pad).fillMaxSize()) {
            if (!ui.loaded) CircularProgressIndicator(Modifier.align(Alignment.Center))
            else when (tab) {
                0 -> KasirScreen(vm)
                1 -> OrdersScreen(vm)
                2 -> TablesScreen(vm, onPickTable = { id -> vm.setTable(id); tab = 0 })
                3 -> HistoryScreen(history, vm)
                else -> ShiftScreen(vm)
            }
        }
    }

    pay?.let { PaymentDialog(vm, it) }
    receipt?.let { ReceiptDialog(vm, it) }
    if (showSettings) SettingsDialog(vm, onDismiss = { showSettings = false }, onLogout = { showSettings = false; vm.logout(); onLoggedOut() })
}
