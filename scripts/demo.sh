#!/usr/bin/env bash
# Populates a fresh .medulla directory with sample robotics lab notes.
# Run from the root of any project directory:
#   bash /path/to/scripts/demo.sh
set -e

echo "Initializing medulla..."
med init

echo "Loading sample memories..."

# --- Sensors + Vision ---
med learn "LiDAR point clouds require pre-filtering to remove ground plane noise before object detection" --tags sensors,vision
med learn "Stereo camera baseline width directly affects depth estimation accuracy at range" --tags sensors,vision
med learn "Time-of-flight sensors saturate in high-ambient-light outdoor environments" --tags sensors,vision

# --- Sensors + Control ---
med learn "IMU drift accumulates over time and must be corrected with external reference during long runs" --tags sensors,control
med learn "Encoder resolution limits minimum controllable velocity at low speeds" --tags sensors,control
med learn "Sensor fusion via Kalman filter reduces state estimation noise by combining IMU and wheel odometry" --tags sensors,control

# --- Actuators + Materials ---
med learn "Silicone-coated grippers conform to irregular surfaces better than rigid fingers" --tags actuators,materials
med learn "Carbon fibre linkages reduce moving mass but require careful adhesive joint design" --tags actuators,materials
med learn "Shape memory alloy actuators provide high force density but require thermal cycling time" --tags actuators,materials

# --- Actuators + Power ---
med learn "Brushless DC motors require ESC tuning to avoid resonance at mid-throttle" --tags actuators,power
med learn "Hydraulic actuators deliver higher peak force but introduce fluid management complexity" --tags actuators,power

# --- Vision + Control ---
med learn "Visual servoing latency above 80ms causes instability in high-speed grasping tasks" --tags vision,control
med learn "Fiducial marker detection provides robust pose estimates in structured environments" --tags vision,control

# --- Materials + Power ---
med learn "Graphene-enhanced battery electrodes increase energy density but complicate recycling" --tags materials,power
med learn "Aerogel thermal insulation keeps battery packs within operating range in cold environments" --tags materials,power

# --- Single-tag entries for breadth ---
med learn "Redundant power rails prevent total system failure from a single regulator fault" --tags power
med learn "Inverse kinematics solvers must handle joint limit constraints to avoid singularities" --tags control
med learn "Depth cameras struggle with transparent and specular surfaces due to IR absorption" --tags vision
med learn "Compliant mechanisms reduce shock loads on gearboxes during hard contact events" --tags actuators

echo "Consolidating..."
med think

echo ""
echo "=== Demo Query: sensors ==="
med query "sensors"

echo ""
echo "=== Demo Query: actuators ==="
med query "actuators"
