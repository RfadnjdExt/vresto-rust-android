package id.vresto.pos.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import id.vresto.pos.data.ApiException
import id.vresto.pos.data.PosRepository
import id.vresto.pos.data.Session
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

data class LoginUi(val busy: Boolean = false, val error: String? = null)

@HiltViewModel
class AuthViewModel @Inject constructor(
    private val repo: PosRepository,
    private val session: Session,
) : ViewModel() {
    private val _ui = MutableStateFlow(LoginUi())
    val ui: StateFlow<LoginUi> = _ui.asStateFlow()

    val savedServer: String get() = session.snapshot.baseUrl.trimEnd('/').removePrefix("http://")
    val hasSession: Boolean get() = session.snapshot.token.isNotEmpty()

    fun login(server: String, user: String, pass: String, onSuccess: () -> Unit) {
        if (server.isBlank() || user.isBlank() || pass.isBlank()) { _ui.value = LoginUi(error = "Lengkapi alamat server, username, dan password."); return }
        viewModelScope.launch {
            _ui.value = LoginUi(busy = true)
            try { repo.login(server, user, pass); _ui.value = LoginUi(); onSuccess() }
            catch (e: ApiException) { _ui.value = LoginUi(error = e.message) }
        }
    }
}
