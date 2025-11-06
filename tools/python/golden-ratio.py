# import uuid
# import math
# from typing import Any, Dict, List

# # φ – the golden ratio
# PHI = (1 + 5 ** 0.5) / 2          # ≈ 1.618033988749895
# INV_PHI = 1 / PHI                # ≈ 0.6180339887498949

# def _golden_weight(index: int) -> float:
#     """
#     Weight that decays geometrically with the golden ratio.
#     * index 0 → 1.0
#     * index 1 → 1/φ
#     * index 2 → 1/φ²
#     ...
#     """
#     return INV_PHI ** index