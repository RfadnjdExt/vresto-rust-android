package id.vresto.pos.data

import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.Json
import retrofit2.HttpException
import java.io.IOException
import javax.inject.Inject
import javax.inject.Singleton

class ApiException(message: String, val code: String? = null, val httpStatus: Int? = null) : Exception(message) {
    val isUnauthorized get() = httpStatus == 401
    val isShiftClosed get() = httpStatus == 409
}

@Singleton
class PosRepository @Inject constructor(
    private val api: VrestoApi,
    private val session: Session,
    private val json: Json,
) {

    private suspend fun <T> guard(block: suspend () -> T): T = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: HttpException) {
        val body = runCatching { e.response()?.errorBody()?.string()?.let { json.decodeFromString<ErrorBody>(it) } }.getOrNull()
        throw ApiException(body?.message ?: "Server menolak permintaan (${e.code()}).", body?.code, e.code())
    } catch (e: IOException) {
        throw ApiException("Tidak dapat terhubung ke server. Periksa Wi-Fi dan alamat server.")
    }

    private suspend fun <T> call(block: suspend () -> Envelope<T>): T = guard {
        val r = block()
        @Suppress("UNCHECKED_CAST")
        if (r.data == null && r.success) (null as T) else r.data as T
    }

    suspend fun login(baseUrl: String, username: String, password: String): UserDto {

        session.saveLogin(Session.normalizeBaseUrl(baseUrl), "")
        val data = call { api.login(LoginReq(username.trim(), password)) }
        session.saveLogin(Session.normalizeBaseUrl(baseUrl), data.token)
        return data.user
    }

    suspend fun logout() = session.clearToken()

    suspend fun init() = call { api.init() }
    suspend fun tables() = call { api.tables() }
    suspend fun stats() = call { api.stats() }
    suspend fun activeOrders() = call { api.orders("active") }
    suspend fun order(id: Long) = call { api.order(id) }
    suspend fun createOrder(req: OrderReq) = call { api.createOrder(req) }
    suspend fun updateOrder(id: Long, req: OrderReq) = call { api.updateOrder(id, req) }
    suspend fun setStatus(id: Long, status: String) = call { api.setStatus(id, StatusReq(status)) }
    suspend fun complete(id: Long) = call { api.complete(id) }
    suspend fun cancel(id: Long) = call { api.cancel(id) }
    suspend fun pay(req: PayReq) = call { api.pay(req) }

    suspend fun currentShift() = call { api.currentShift() }
    suspend fun startShift(openingCash: Long, notes: String?) = call { api.startShift(StartShiftReq(openingCash, notes)) }
    suspend fun closeShift(actual: Long, notes: String?) = call { api.closeShift(CloseShiftReq(actual, notes)) }
    suspend fun cashMove(type: String, amount: Long, desc: String?) = call { api.cashMove(CashMoveReq(type, amount, desc)) }
    suspend fun expenses() = call { api.expenses() }
    suspend fun addExpense(category: String, desc: String?, amount: Long) = call { api.addExpense(ExpenseReq(category, desc, amount)) }

    suspend fun transactions(date: String?, endDate: String?, status: String?, search: String?) =
        guard { api.transactions(date, endDate, status?.takeIf { it != "all" }, search?.takeIf { it.isNotBlank() }) }
    suspend fun transaction(id: Long) = call { api.transaction(id) }
    suspend fun voidTransaction(id: Long, reason: String) = call { api.voidTransaction(id, VoidReq(reason)) }
    suspend fun refund(id: Long, req: RefundReq) = call { api.refundTransaction(id, req) }

    suspend fun debts(status: String, customerId: Long? = null) = call { api.debts(status, customerId) }
    suspend fun debt(id: Long) = call { api.debt(id) }
    suspend fun repayDebt(id: Long, req: RepayReq) = call { api.repayDebt(id, req) }
    suspend fun repayCustomer(customerId: Long, req: RepayReq) = call { api.repayCustomer(customerId, req) }
    suspend fun reversePayment(paymentId: Long, reason: String) = call { api.reversePayment(paymentId, ReverseReq(reason)) }
}
