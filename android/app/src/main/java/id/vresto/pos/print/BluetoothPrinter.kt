package id.vresto.pos.print

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import id.vresto.pos.data.Session
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

data class PrinterDevice(val name: String, val mac: String)

@Singleton
class BluetoothPrinter @Inject constructor(
    @ApplicationContext private val ctx: Context,
    private val session: Session,
) {
    private val spp: UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")
    private val adapter get() = ctx.getSystemService(BluetoothManager::class.java)?.adapter

    fun hasPermission(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED

    @SuppressLint("MissingPermission")
    fun pairedDevices(): List<PrinterDevice> =
        if (!hasPermission()) emptyList()
        else adapter?.bondedDevices.orEmpty().map { PrinterDevice(it.name ?: it.address, it.address) }.sortedBy { it.name }

    @SuppressLint("MissingPermission")
    suspend fun print(bytes: ByteArray): Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            val mac = session.snapshot.printerMac
            require(mac.isNotEmpty()) { "Printer belum dipilih. Pilih di Pengaturan." }
            check(hasPermission()) { "Izin Bluetooth belum diberikan." }
            val bt = checkNotNull(adapter) { "Perangkat ini tidak punya Bluetooth." }
            check(bt.isEnabled) { "Bluetooth mati. Nyalakan dulu." }
            bt.cancelDiscovery()
            val socket = bt.getRemoteDevice(mac).createRfcommSocketToServiceRecord(spp)
            try {
                socket.connect()
                val os = socket.outputStream

                bytes.toList().chunked(256).forEach { os.write(it.toByteArray()); os.flush(); delay(20) }
                delay(300)
            } finally {
                runCatching { socket.close() }
            }
        }.fold(
            onSuccess = { Result.success(Unit) },
            onFailure = { Result.failure(Exception(it.message ?: "Gagal mencetak. Pastikan printer menyala dan dalam jangkauan.")) },
        )
    }
}
