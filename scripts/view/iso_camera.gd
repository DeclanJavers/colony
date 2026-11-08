class_name IsoCamera
extends Node3D
## Orthographic isometric camera rig.
##   WASD / arrows / right- or middle-drag: pan    Wheel: zoom    Q / E: rotate 90°    F: focus selection

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


func focus(p: Vector3) -> void:
	position = Vector3(p.x, 0, p.z)


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
		var ax := _ground_axes()
		position += (ax[0] * dir.x + ax[1] * dir.y) * camera.size * pan_speed * delta
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
	elif event is InputEventMouseMotion and _dragging:
		var ax := _ground_axes()
		var px_to_world := camera.size / float(get_viewport().get_visible_rect().size.y)
		# Moving the mouse drags the ground with it; vertical motion is foreshortened by the pitch.
		position -= ax[0] * event.relative.x * px_to_world
		position += ax[1] * event.relative.y * px_to_world / sin(deg_to_rad(-PITCH_DEG))
	elif event is InputEventKey and event.pressed and not event.echo:
		match event.keycode:
			KEY_Q:
				_yaw_target += PI / 2.0
			KEY_E:
				_yaw_target -= PI / 2.0
