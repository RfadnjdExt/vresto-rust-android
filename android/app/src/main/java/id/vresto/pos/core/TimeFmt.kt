package id.vresto.pos.core

import java.time.OffsetDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter

private val FULL = DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm")
private val HOUR = DateTimeFormatter.ofPattern("HH:mm")

fun formatLocal(iso: String?, zone: ZoneId = ZoneId.systemDefault()): String =
    runCatching { OffsetDateTime.parse(iso).atZoneSameInstant(zone).format(FULL) }.getOrDefault(iso.orEmpty())

fun formatHour(iso: String?, zone: ZoneId = ZoneId.systemDefault()): String =
    runCatching { OffsetDateTime.parse(iso).atZoneSameInstant(zone).format(HOUR) }.getOrDefault("")
