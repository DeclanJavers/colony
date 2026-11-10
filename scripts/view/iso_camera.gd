class_name IsoCamera
extends Node3D
## Orthographic isometric camera rig.
##   Drag (left, right or middle) / WASD / arrows / two-finger swipe: pan
##   Wheel / pinch: zoom    Q / E: rotate 90°    F: focus selection
## A left press only becomes a drag once it moves a few pixels, so clicks still select things.

const PITCH_DEG := -35.264   # true isometric
const MIN_SIZE := 6.0
const MAX_SIZE := 70.0

var camera := Camera3D.new()
var bounds := Rect2(0, 0, 60, 60)
var pan_speed := 1.2           # screen-heights per second
var _pitch := Node3D.new()
var _yaw_target := deg_to_rad(45.0)
var _size_target := 18.0
var _dragging := false
var _left_down := false
var _left_from := Vector2.ZERO
const DRAG_START_PX := 6.0
const PAN_GESTURE_PX := 20.0      # screen pixels per unit of trackpad swipe
## The current left press has turned into a drag (WorldView doesn't select on its release).
static var left_dragged := false
var _glide: Variant = null      # Vector3 the camera is gliding to
var follow_node: Node3D = null    # when set, the camera glides after it (any manual pan stops it)


func _ready() -> void:
	add_child(_pitch)
	_pitch.rotation.x = deg_to_rad(PITCH_DEG)
	_pitch.add_child(camera)
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = _size_target
	camera.position = Vector3(0, 0, 80)
	camera.near = 0.1
	camera.far = 300.0
	camera.current = true
	rotation.y = _yaw_target


## Glide to a point (instant=true jumps). Any manual pan cancels the glide.
func focus(p: Vector3, instant := false) -> void:
	if instant:
		position = Vector3(p.x, 0, p.z)
		_glide = null
	else:
		_glide = Vector3(p.x, 0, p.z)


## Move as if the ground were dragged by `rel` screen pixels (vertical is foreshortened by the pitch).
func _drag_by(rel: Vector2) -> void:
	follow_node = null
	_glide = null
	var ax := _ground_axes()
	var px_to_world := camera.size / float(get_viewport().get_visible_rect().size.y)
	position -= ax[0] * rel.x * px_to_world
	position += ax[1] * rel.y * px_to_world / sin(deg_to_rad(-PITCH_DEG))


func _ground_axes() -> Array[Vector3]:
	var right := global_transform.basis.x
	var fwd := -global_transform.basis.z
	right.y = 0
	fwd.y = 0
	return [right.normalized(), fwd.normalized()]


func _process(delta: float) -> void:
	var dir := Vector2.ZERO
	var typing := get_viewport().gui_get_focus_owner() is LineEdit
	if not typing and (Input.is_key_pressed(KEY_A) or Input.is_key_pressed(KEY_LEFT)):
		dir.x -= 1
	if not typing and (Input.is_key_pressed(KEY_D) or Input.is_key_pressed(KEY_RIGHT)):
		dir.x += 1
	if not typing and (Input.is_key_pressed(KEY_W) or Input.is_key_pressed(KEY_UP)):
		dir.y += 1
	if not typing and (Input.is_key_pressed(KEY_S) or Input.is_key_pressed(KEY_DOWN)):
		dir.y -= 1
	if dir != Vector2.ZERO:
		follow_node = null
		_glide = null
		var ax := _ground_axes()
		position += (ax[0] * dir.x + ax[1] * dir.y) * camera.size * pan_speed * delta
	if follow_node != null and is_instance_valid(follow_node):
		var p := follow_node.global_position
		position = position.lerp(Vector3(p.x, 0, p.z), clampf(delta * 4.0, 0, 1))
	elif follow_node != null:
		follow_node = null
	elif _glide != null:
		position = position.lerp(_glide, clampf(delta * 6.0, 0, 1))
		if position.distance_to(_glide) < 0.02:
			_glide = null
	rotation.y = lerp_angle(rotation.y, _yaw_target, clampf(delta * 10.0, 0, 1))
	camera.size = lerpf(camera.size, _size_target, clampf(delta * 12.0, 0, 1))
	position.x = clampf(position.x, bounds.position.x, bounds.end.x)
	position.z = clampf(position.z, bounds.position.y, bounds.end.y)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		match event.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				_size_target = clampf(_size_target * 0.9, MIN_SIZE, MAX_SIZE)
			MOUSE_BUTTON_WHEEL_DOWN:
				_size_target = clampf(_size_target * 1.1, MIN_SIZE, MAX_SIZE)
			MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE:
				_dragging = event.pressed
			MOUSE_BUTTON_LEFT:
				_left_down = event.pressed
				if event.pressed:
					_left_from = event.position
					left_dragged = false
	elif event is InputEventMouseMotion and _left_down and not left_dragged \
			and event.position.distance_to(_left_from) > DRAG_START_PX:
		left_dragged = true
		_drag_by(event.position - _left_from)
	elif event is InputEventMouseMotion and (_dragging or left_dragged and _left_down):
		_drag_by(event.relative)
	elif event is InputEventPanGesture:        # trackpad: two-finger swipe
		_drag_by(-event.delta * PAN_GESTURE_PX)
	elif event is InputEventMagnifyGesture:    # trackpad: pinch
		_size_target = clampf(_size_target / maxf(event.factor, 0.01), MIN_SIZE, MAX_SIZE)
	elif event is InputEventKey and event.pressed and not event.echo:
		match event.keycode:
			KEY_Q:
				_yaw_target += PI / 2.0
			KEY_E:
				_yaw_target -= PI / 2.0
