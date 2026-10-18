#pragma once
#include <algorithm>
#include <stdexcept>

// Call immediately after submission, before any output is consumed.
inline unsigned pending_peak(unsigned peak, unsigned submitted, unsigned completed) {
    if (completed > submitted) throw std::invalid_argument("Output exceeds submitted input");
    return (std::max)(peak, submitted - completed);
}
