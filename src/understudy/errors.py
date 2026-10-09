"""Errors shared by the command line, transport and viewer."""


class UnderstudyError(RuntimeError):
    def __init__(self, message, code="unavailable", uncertain=False):
        super().__init__(message)
        self.code = code
        self.uncertain = uncertain

    def as_dict(self):
        result = {"ok": False, "code": self.code, "error": str(self)}
        if self.uncertain:
            result["delivery"] = "unknown; do not replay. Observe, then restart the helper before further control."
        return result


class SessionError(UnderstudyError):
    pass
