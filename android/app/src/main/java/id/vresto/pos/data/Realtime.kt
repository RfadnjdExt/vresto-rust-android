package id.vresto.pos.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import javax.inject.Inject
import javax.inject.Singleton

data class RealtimeEvent(val name: String, val data: JsonObject? = null)

@Singleton
class RealtimeClient @Inject constructor(
    private val http: OkHttpClient,
    private val session: Session,
    private val json: Json,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val _events = MutableSharedFlow<RealtimeEvent>(extraBufferCapacity = 64)
    val events: SharedFlow<RealtimeEvent> = _events

    private var job: Job? = null
    @Volatile private var socket: WebSocket? = null

    fun start() {
        if (job?.isActive == true) return
        job = scope.launch {
            var backoff = 1_000L
            while (isActive) {
                val snap = session.snapshot
                if (snap.token.isEmpty() || snap.baseUrl.isEmpty()) { delay(2_000); continue }
                val wsUrl = snap.baseUrl.replaceFirst("http", "ws") + "api/v1/ws?token=" + snap.token
                val closed = kotlinx.coroutines.CompletableDeferred<Boolean>()
                var opened = false
                socket = http.newWebSocket(Request.Builder().url(wsUrl).build(), object : WebSocketListener() {
                    override fun onOpen(ws: WebSocket, response: Response) {
                        opened = true
                        _events.tryEmit(RealtimeEvent("connected"))
                        _events.tryEmit(RealtimeEvent("resync"))
                    }
                    override fun onMessage(ws: WebSocket, text: String) {
                        runCatching {
                            val o = json.parseToJsonElement(text).jsonObject
                            _events.tryEmit(RealtimeEvent(o["event"]!!.jsonPrimitive.content, o["data"] as? JsonObject))
                        }
                    }
                    override fun onClosed(ws: WebSocket, code: Int, reason: String) { closed.complete(opened) }
                    override fun onFailure(ws: WebSocket, t: Throwable, response: Response?) { closed.complete(opened) }
                })
                val wasConnected = closed.await()
                _events.tryEmit(RealtimeEvent("disconnected"))
                backoff = if (wasConnected) 1_000L else (backoff * 2).coerceAtMost(30_000L)
                delay(backoff)
            }
        }
    }

    fun stop() {
        job?.cancel(); job = null
        socket?.close(1000, "logout"); socket = null
    }
}
