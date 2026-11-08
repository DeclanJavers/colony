class_name ResNode
extends RefCounted
## A natural resource on the map: tree, rock, berry bush. Type data comes from data/nodes.json.

var id: int
var type: String
var cell: Vector2i
var amount: float
var max_amount: float
var workers: int = 0   # how many colonists are currently targeting it


func def() -> Dictionary:
	return Content.get_instance().nodes[type]


func to_dict() -> Dictionary:
	return {"id": id, "type": type, "cell": [cell.x, cell.y], "amount": amount}
