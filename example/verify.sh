#!/bin/bash

# 1. Environment Setup
echo "--- Initializing Medulla Test Environment ---"
rm -rf .medulla
../target/release/med init

# 2. PHASE 1: Legacy Knowledge (High Volume, Low Recency)
# We simulate these as if they were learned earlier in the project lifecycle.
echo -e "\n--- Learning Legacy Data: Greenhouse Automation ---"
../target/release/med learn "Calibrating soil moisture sensors for the tomato bed." --tags agriculture,sensors,arduino
 ../target/release/med learn "Arduino code for the automated watering pump controller." --tags agriculture,arduino
 ../target/release/med learn "Switching to I2C sensors to reduce wiring complexity." --tags agriculture,sensors
 ../target/release/med learn "Solar panel efficiency tests for the greenhouse roof." --tags agriculture,solar

# 3. PHASE 2: Active Research (Low Volume, High Recency)
# These represent your current 'Working Memory' focus.
echo -e "\n--- Learning Active Data: Octopod Robotics ---"
../target/release/med learn "Molding the first silicone tentacle for the Octopod limb." --tags robotics,silicone,actuators
 ../target/release/med learn "Testing pneumatic actuators for soft-body movement." --tags robotics,actuators

# 4. PHASE 3: The Hebbian Bridge
# This entry specifically links 'sensors' (from Agriculture) with 'silicone' (from Robotics).
echo -e "\n--- Learning The Bridge: Materials Science ---"
../target/release/med learn "Integrating flexible sensors directly into the silicone skin." --tags sensors,silicone,materials

# 5. PHASE 4: The Noise (Unrelated)
echo -e "\n--- Learning Noise Data: Cooking ---"
../target/release/med learn "Sourdough hydration ratio set to 75% for better crumb." --tags cooking,recipe

# 6. CONSOLIDATION (The "Think" Phase)
# This is where the Reconstructor builds the Parquet indices and applies the decay.
echo -e "\n--- Consolidating Memories (Running 'think') ---"
../target/release/med think

# 7. VERIFICATION
echo -e "\n=========================================="
echo "VERIFICATION RESULTS"
echo "=========================================="

# TEST 1: Recency Bias
# Even though Agriculture has 4 entries and Robotics has 2,
# Robotics should have higher Activation scores because it was learned last.
echo -e "\n[TEST 1] Recency Check: Searching for 'robot'"
../target/release/med query "robot" --limit 3

# TEST 2: Hebbian Association
# Searching for 'agriculture' should now show 'sensors' as a strong link,
# but the 'Related Concepts' section should also surface 'silicone'
# because of the Bridge entry.

echo -e "\n[TEST 2] Hebbian Check: Searching for 'agriculture'"
../target/release/med query "agriculture" --limit 1

# TEST 3: Semantic Bridge
# Searching for 'silicone' should show 'robotics' and 'sensors' as top associates.
echo -e "\n[TEST 3] Bridge Check: Searching for 'silicone'"
../target/release/med query "silicone" --limit 1

# TEST 4: Noise Isolation
# Searching for 'recipe' should show NO links to robotics or agriculture.
echo -e "\n[TEST 4] Noise Check: Searching for 'recipe'"
../target/release/med query "recipe" --limit 1

echo -e "\nVerification Complete."
