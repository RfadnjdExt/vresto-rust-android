package id.vresto.pos.data

import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.PATCH
import retrofit2.http.POST
import retrofit2.http.PUT
import retrofit2.http.Path
import retrofit2.http.Query
import retrofit2.http.DELETE

interface VrestoApi {
    @POST("api/v1/auth/login") suspend fun login(@Body body: LoginReq): Envelope<LoginData>

    @GET("api/v1/pos/init") suspend fun init(): Envelope<InitDto>
    @GET("api/v1/pos/tables") suspend fun tables(): Envelope<List<TableDto>>
    @GET("api/v1/pos/products") suspend fun products(@Query("q") q: String? = null): Envelope<List<ProductDto>>
    @GET("api/v1/pos/dashboard/stats") suspend fun stats(): Envelope<StatsDto>

    @GET("api/v1/pos/orders") suspend fun orders(@Query("status") status: String = "active"): Envelope<List<OrderDto>>
    @GET("api/v1/pos/orders/{id}") suspend fun order(@Path("id") id: Long): Envelope<OrderDto>
    @POST("api/v1/pos/orders") suspend fun createOrder(@Body body: OrderReq): Envelope<OrderDto>
    @PUT("api/v1/pos/orders/{id}") suspend fun updateOrder(@Path("id") id: Long, @Body body: OrderReq): Envelope<OrderDto>
    @PATCH("api/v1/pos/orders/{id}/status") suspend fun setStatus(@Path("id") id: Long, @Body body: StatusReq): Envelope<OrderDto>
    @POST("api/v1/pos/orders/{id}/complete") suspend fun complete(@Path("id") id: Long): Envelope<OrderDto>
    @POST("api/v1/pos/orders/{id}/cancel") suspend fun cancel(@Path("id") id: Long): Envelope<OrderDto>

    @POST("api/v1/pos/transactions") suspend fun pay(@Body body: PayReq): Envelope<PayResultDto>

    @GET("api/v1/pos/transactions") suspend fun transactions(
        @Query("date") date: String?, @Query("end_date") endDate: String?,
        @Query("status") status: String?, @Query("search") search: String?,
    ): TransactionListResponse
    @GET("api/v1/pos/transactions/{id}") suspend fun transaction(@Path("id") id: Long): Envelope<TransactionDto>
    @POST("api/v1/pos/transactions/{id}/void") suspend fun voidTransaction(@Path("id") id: Long, @Body body: VoidReq): Envelope<TransactionDto>
    @POST("api/v1/pos/transactions/{id}/refund") suspend fun refundTransaction(@Path("id") id: Long, @Body body: RefundReq): Envelope<RefundResultDto>

    @GET("api/v1/pos/debts") suspend fun debts(@Query("status") status: String, @Query("customer_id") customerId: Long? = null): Envelope<List<DebtDto>>
    @GET("api/v1/pos/debts/{id}") suspend fun debt(@Path("id") id: Long): Envelope<DebtDto>
    @POST("api/v1/pos/debts/{id}/payments") suspend fun repayDebt(@Path("id") id: Long, @Body body: RepayReq): Envelope<RepayResultDto>
    @POST("api/v1/pos/customers/{id}/debt-payments") suspend fun repayCustomer(@Path("id") id: Long, @Body body: RepayReq): Envelope<RepayResultDto>
    @POST("api/v1/pos/debt-payments/{id}/reverse") suspend fun reversePayment(@Path("id") id: Long, @Body body: ReverseReq): Envelope<ReverseResultDto>

    @GET("api/v1/pos/shifts/current") suspend fun currentShift(): Envelope<ShiftSummaryDto?>
    @POST("api/v1/pos/shifts/start") suspend fun startShift(@Body body: StartShiftReq): Envelope<ShiftSummaryDto>
    @POST("api/v1/pos/shifts/close") suspend fun closeShift(@Body body: CloseShiftReq): Envelope<ShiftSummaryDto>
    @POST("api/v1/pos/shifts/cash-movement") suspend fun cashMove(@Body body: CashMoveReq): Envelope<ShiftSummaryDto>

    @GET("api/v1/pos/expenses") suspend fun expenses(): Envelope<List<ExpenseDto>>
    @POST("api/v1/pos/expenses") suspend fun addExpense(@Body body: ExpenseReq): Envelope<ExpenseDto>
    @DELETE("api/v1/pos/expenses/{id}") suspend fun deleteExpense(@Path("id") id: Long): Envelope<kotlinx.serialization.json.JsonElement?>
}
