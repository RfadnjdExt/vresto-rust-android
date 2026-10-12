package id.vresto.pos.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import id.vresto.pos.BuildConfig
import id.vresto.pos.data.Session
import id.vresto.pos.data.VrestoApi
import retrofit2.converter.kotlinx.serialization.asConverterFactory
import kotlinx.serialization.json.Json
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.logging.HttpLoggingInterceptor
import retrofit2.Retrofit
import java.util.concurrent.TimeUnit
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object NetworkModule {
    @Provides @Singleton
    fun json(): Json = Json { ignoreUnknownKeys = true; coerceInputValues = true; explicitNulls = false; encodeDefaults = false }

    @Provides @Singleton
    fun okHttp(session: Session): OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(8, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .pingInterval(15, TimeUnit.SECONDS)

        .addInterceptor { chain ->
            val snap = session.snapshot
            var req = chain.request()
            snap.baseUrl.toHttpUrlOrNull()?.let { base ->
                req = req.newBuilder()
                    .url(req.url.newBuilder().scheme(base.scheme).host(base.host).port(base.port).build())
                    .build()
            }
            if (snap.token.isNotEmpty()) req = req.newBuilder().header("Authorization", "Bearer ${snap.token}").build()
            chain.proceed(req)
        }
        .apply { if (BuildConfig.DEBUG) addInterceptor(HttpLoggingInterceptor().apply { level = HttpLoggingInterceptor.Level.BASIC }) }
        .build()

    @Provides @Singleton
    fun retrofit(client: OkHttpClient, json: Json): Retrofit = Retrofit.Builder()
        .baseUrl("http://placeholder.invalid/")
        .client(client)
        .addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
        .build()

    @Provides @Singleton
    fun api(retrofit: Retrofit): VrestoApi = retrofit.create(VrestoApi::class.java)
}
