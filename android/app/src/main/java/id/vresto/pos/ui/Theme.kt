package id.vresto.pos.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val Orange = Color(0xFFE65100)
private val Light = lightColorScheme(primary = Orange, secondary = Color(0xFF5D4037), tertiary = Color(0xFF2E7D32))
private val Dark = darkColorScheme(primary = Color(0xFFFFB74D), secondary = Color(0xFFBCAAA4), tertiary = Color(0xFF81C784))

@Composable
fun VrestoTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) Dark else Light, content = content)
}
