package id.vresto.pos.data

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import javax.inject.Inject
import javax.inject.Singleton

private val Context.store by preferencesDataStore("session")

@Singleton
class Session @Inject constructor(@ApplicationContext private val ctx: Context) {
    data class Snapshot(val baseUrl: String = "", val token: String = "", val printerMac: String = "")

    private val kBase = stringPreferencesKey("base_url")
    private val kToken = stringPreferencesKey("token")
    private val kPrinter = stringPreferencesKey("printer_mac")

    val flow: Flow<Snapshot> = ctx.store.data.map {
        Snapshot(it[kBase].orEmpty(), it[kToken].orEmpty(), it[kPrinter].orEmpty())
    }

    @Volatile var snapshot: Snapshot = runBlocking { flow.first() }
        private set

    suspend fun saveLogin(baseUrl: String, token: String) {
        ctx.store.edit { it[kBase] = baseUrl; it[kToken] = token }
        snapshot = snapshot.copy(baseUrl = baseUrl, token = token)
    }

    suspend fun savePrinter(mac: String) {
        ctx.store.edit { it[kPrinter] = mac }
        snapshot = snapshot.copy(printerMac = mac)
    }

    suspend fun clearToken() {
        ctx.store.edit { it[kToken] = "" }
        snapshot = snapshot.copy(token = "")
    }

    companion object {
        fun normalizeBaseUrl(raw: String): String {
            val t = raw.trim().trimEnd('/')
            val withScheme = if (t.startsWith("http://") || t.startsWith("https://")) t else "http://$t"
            return "$withScheme/"
        }
    }
}
