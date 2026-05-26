var __webby_nodes = {};
var __webby_mutations = [];
var __webby_event_handlers = [];
var __webby_next_temp_id = 1;

function __webby_install(node, parent) {
  __webby_nodes[node.id] = node;
  node.parent = parent;
  for (var index = 0; index < node.children.length; index = index + 1) {
    __webby_install(node.children[index], node.id);
  }
}

var __webby_geometry = {};
for (var __webby_geometry_index = 0; __webby_geometry_index < __webby_geometry_entries.length; __webby_geometry_index = __webby_geometry_index + 1) {
  var __webby_geometry_entry = __webby_geometry_entries[__webby_geometry_index];
  __webby_geometry[String(__webby_geometry_entry.node_id)] = __webby_geometry_entry;
}

function __webby_new_temp_id() {
  var id = "new" + __webby_next_temp_id;
  __webby_next_temp_id = __webby_next_temp_id + 1;
  return id;
}

function __webby_text_content(node) {
  if (!node) {
    return "";
  }
  if (node.kind === "text") {
    return node.text;
  }
  var output = "";
  for (var index = 0; index < node.children.length; index = index + 1) {
    output = output + __webby_text_content(node.children[index]);
  }
  return output;
}

function __webby_set_text_content(id, value) {
  var node = __webby_nodes[id];
  if (!node) {
    throw new Error("missing DOM node " + id);
  }
  if (node.kind === "text") {
    node.text = value;
  } else {
    node.children = [];
  }
  __webby_mutations.push({ op: "setTextContent", id: id, text: value });
}

function __webby_set_attribute(id, name, value) {
  var node = __webby_nodes[id];
  if (!node || node.kind !== "element") {
    throw new Error("missing DOM element " + id);
  }
  node.attributes[String(name).toLowerCase()] = String(value);
  __webby_mutations.push({
    op: "setAttribute",
    id: id,
    name: String(name),
    value: String(value)
  });
}

function __webby_remove_attribute(id, name) {
  var node = __webby_nodes[id];
  if (!node || node.kind !== "element") {
    throw new Error("missing DOM element " + id);
  }
  delete node.attributes[String(name).toLowerCase()];
  __webby_mutations.push({
    op: "setAttribute",
    id: id,
    name: String(name),
    value: ""
  });
}

function __webby_get_attribute(id, name) {
  var node = __webby_nodes[id];
  if (!node || node.kind !== "element") {
    return null;
  }
  var key = String(name).toLowerCase();
  if (!Object.prototype.hasOwnProperty.call(node.attributes, key)) {
    return null;
  }
  return node.attributes[key];
}

function __webby_detach(id) {
  var node = __webby_nodes[id];
  if (!node || node.parent === null) {
    return;
  }
  var parent = __webby_nodes[node.parent];
  if (!parent) {
    return;
  }
  var kept = [];
  for (var index = 0; index < parent.children.length; index = index + 1) {
    if (parent.children[index].id !== id) {
      kept.push(parent.children[index]);
    }
  }
  parent.children = kept;
  node.parent = null;
}

function __webby_append_child(parentId, childRef) {
  var parent = __webby_nodes[parentId];
  var child = childRef && __webby_nodes[childRef.__webbyId];
  if (!parent || parent.kind === "text") {
    throw new Error("missing DOM parent " + parentId);
  }
  if (!child) {
    throw new Error("missing DOM child");
  }
  __webby_detach(child.id);
  child.parent = parent.id;
  parent.children.push(child);
  __webby_mutations.push({
    op: "appendChild",
    parent: parent.id,
    child: child.id
  });
  return childRef;
}

function __webby_child_refs(node, elementsOnly) {
  var output = [];
  if (!node) {
    return output;
  }
  for (var index = 0; index < node.children.length; index = index + 1) {
    var child = node.children[index];
    if (!elementsOnly || child.kind === "element") {
      output.push(__webby_ref(child.id));
    }
  }
  return output;
}

function __webby_sibling_ref(id, direction) {
  var node = __webby_nodes[id];
  var parent = node && __webby_nodes[node.parent];
  if (!parent) {
    return null;
  }
  for (var index = 0; index < parent.children.length; index = index + 1) {
    if (parent.children[index].id === id) {
      var sibling = parent.children[index + direction];
      return sibling ? __webby_ref(sibling.id) : null;
    }
  }
  return null;
}

function __webby_node_name(node) {
  if (!node) {
    return "";
  }
  if (node.kind === "document") {
    return "#document";
  }
  if (node.kind === "text") {
    return "#text";
  }
  return node.tag_name.toUpperCase();
}

function __webby_dataset_key_to_attr(key) {
  var output = "data-";
  var string = String(key);
  for (var index = 0; index < string.length; index = index + 1) {
    var ch = string.charAt(index);
    if (ch >= "A" && ch <= "Z") {
      output = output + "-" + ch.toLowerCase();
    } else {
      output = output + ch;
    }
  }
  return output;
}

function __webby_attr_to_dataset_key(name) {
  var rest = String(name).slice(5);
  var output = "";
  var upperNext = false;
  for (var index = 0; index < rest.length; index = index + 1) {
    var ch = rest.charAt(index);
    if (ch === "-") {
      upperNext = true;
    } else if (upperNext) {
      output = output + ch.toUpperCase();
      upperNext = false;
    } else {
      output = output + ch;
    }
  }
  return output;
}

function __webby_make_dataset(id) {
  var initial = {};
  var node = __webby_nodes[id];
  if (node && node.kind === "element") {
    for (var name in node.attributes) {
      if (Object.prototype.hasOwnProperty.call(node.attributes, name) && name.indexOf("data-") === 0) {
        initial[__webby_attr_to_dataset_key(name)] = node.attributes[name];
      }
    }
  }
  return new Proxy(initial, {
    get: function(target, key) {
      if (typeof key !== "string") {
        return target[key];
      }
      var value = __webby_get_attribute(id, __webby_dataset_key_to_attr(key));
      return value === null ? undefined : value;
    },
    set: function(target, key, value) {
      __webby_set_attribute(id, __webby_dataset_key_to_attr(key), String(value));
      target[key] = String(value);
      return true;
    }
  });
}

function __webby_classes(id) {
  var value = __webby_get_attribute(id, "class") || "";
  var raw = value.split(/\s+/);
  var output = [];
  for (var index = 0; index < raw.length; index = index + 1) {
    if (raw[index] !== "" && output.indexOf(raw[index]) < 0) {
      output.push(raw[index]);
    }
  }
  return output;
}

function __webby_set_classes(id, classes) {
  __webby_set_attribute(id, "class", classes.join(" "));
}

function __webby_make_class_list(id) {
  return {
    contains: function(name) {
      return __webby_classes(id).indexOf(String(name)) >= 0;
    },
    add: function() {
      var classes = __webby_classes(id);
      for (var index = 0; index < arguments.length; index = index + 1) {
        var value = String(arguments[index]);
        if (value !== "" && classes.indexOf(value) < 0) {
          classes.push(value);
        }
      }
      __webby_set_classes(id, classes);
    },
    remove: function() {
      var classes = __webby_classes(id);
      for (var index = 0; index < arguments.length; index = index + 1) {
        var value = String(arguments[index]);
        var next = [];
        for (var classIndex = 0; classIndex < classes.length; classIndex = classIndex + 1) {
          if (classes[classIndex] !== value) {
            next.push(classes[classIndex]);
          }
        }
        classes = next;
      }
      __webby_set_classes(id, classes);
    },
    toggle: function(name) {
      var value = String(name);
      var classes = __webby_classes(id);
      if (classes.indexOf(value) >= 0) {
        this.remove(value);
        return false;
      }
      this.add(value);
      return true;
    },
    get value() {
      return __webby_classes(id).join(" ");
    },
    toString: function() {
      return __webby_classes(id).join(" ");
    }
  };
}

function __webby_parse_style_attribute(value) {
  var output = {};
  var declarations = String(value || "").split(";");
  for (var index = 0; index < declarations.length; index = index + 1) {
    var declaration = declarations[index];
    var colon = declaration.indexOf(":");
    if (colon <= 0) {
      continue;
    }
    output[declaration.slice(0, colon).trim().toLowerCase()] = declaration.slice(colon + 1).trim();
  }
  return output;
}

function __webby_serialize_style_attribute(styles) {
  var keys = Object.keys(styles).sort();
  var output = [];
  for (var index = 0; index < keys.length; index = index + 1) {
    if (styles[keys[index]] !== "") {
      output.push(keys[index] + ": " + styles[keys[index]]);
    }
  }
  return output.join("; ");
}

function __webby_css_property_name(name) {
  return String(name).replace(/[A-Z]/g, function(ch) { return "-" + ch.toLowerCase(); }).toLowerCase();
}

function __webby_make_style(id) {
  function read() {
    return __webby_parse_style_attribute(__webby_get_attribute(id, "style") || "");
  }
  function write(styles) {
    __webby_set_attribute(id, "style", __webby_serialize_style_attribute(styles));
  }
  var api = {
    get cssText() {
      return __webby_get_attribute(id, "style") || "";
    },
    set cssText(value) {
      __webby_set_attribute(id, "style", String(value));
    },
    getPropertyValue: function(name) {
      var styles = read();
      return styles[__webby_css_property_name(name)] || "";
    },
    setProperty: function(name, value) {
      var styles = read();
      styles[__webby_css_property_name(name)] = String(value);
      write(styles);
    },
    removeProperty: function(name) {
      var styles = read();
      var key = __webby_css_property_name(name);
      var old = styles[key] || "";
      delete styles[key];
      write(styles);
      return old;
    }
  };
  var properties = ["color", "backgroundColor", "fontSize", "display", "visibility", "width", "height", "margin", "padding", "border"];
  for (var index = 0; index < properties.length; index = index + 1) {
    (function(property) {
      Object.defineProperty(api, property, {
        get: function() {
          return api.getPropertyValue(__webby_css_property_name(property));
        },
        set: function(value) {
          api.setProperty(__webby_css_property_name(property), value);
        }
      });
    })(properties[index]);
  }
  return api;
}

function __webby_dom_rect(id) {
  var node = __webby_nodes[id];
  var geometry = __webby_geometry[id];
  if (!geometry && node && node.kind === "element" && (node.tag_name === "html" || node.tag_name === "body")) {
    geometry = { x: 0, y: 0, width: __webby_viewport_width, height: __webby_viewport_height };
  }
  var x = geometry ? Number(geometry.x) : 0;
  var y = geometry ? Number(geometry.y) : 0;
  var width = geometry ? Number(geometry.width) : 0;
  var height = geometry ? Number(geometry.height) : 0;
  return {
    x: x,
    y: y,
    width: width,
    height: height,
    top: y,
    left: x,
    right: x + width,
    bottom: y + height
  };
}

function __webby_remove(id) {
  if (!__webby_nodes[id]) {
    return;
  }
  __webby_detach(id);
  __webby_mutations.push({ op: "remove", id: id });
}

function __webby_ref(id) {
  var ref = { __webbyId: id };
  Object.defineProperty(ref, "nodeType", {
    get: function() {
      var node = __webby_nodes[id];
      if (!node) {
        return 0;
      }
      return node.kind === "element" ? 1 : (node.kind === "text" ? 3 : 9);
    }
  });
  Object.defineProperty(ref, "nodeName", {
    get: function() {
      return __webby_node_name(__webby_nodes[id]);
    }
  });
  ref.getContext = function(kind) {
    var node = __webby_nodes[id];
    if (!node || node.kind !== "element" || node.tag_name !== "canvas") {
      return null;
    }
    if (String(kind) !== "2d") {
      throw new Error("canvas only supports 2d context");
    }
    var commands = [];
    var existing = node.attributes["data-webby-canvas"] || "";
    if (existing !== "") {
      var existingCommands = existing.split(";");
      for (var existingIndex = 0; existingIndex < existingCommands.length; existingIndex = existingIndex + 1) {
        if (existingCommands[existingIndex] !== "") {
          commands.push(existingCommands[existingIndex].split(","));
        }
      }
    }
    var context = {
      fillStyle: "black",
      strokeStyle: "black",
      lineWidth: 1
    };
    function number(value) {
      var parsed = Number(value);
      if (!isFinite(parsed)) {
        throw new Error("canvas coordinate must be finite");
      }
      return parsed;
    }
    function serialize() {
      var parts = [];
      for (var index = 0; index < commands.length; index = index + 1) {
        parts.push(commands[index].join(","));
      }
      __webby_set_attribute(id, "data-webby-canvas", parts.join(";"));
    }
    context.fillRect = function(x, y, width, height) {
      commands.push(["fillRect", number(x), number(y), number(width), number(height), String(context.fillStyle)]);
      serialize();
    };
    context.strokeRect = function(x, y, width, height) {
      commands.push(["strokeRect", number(x), number(y), number(width), number(height), String(context.strokeStyle), number(context.lineWidth)]);
      serialize();
    };
    context.clearRect = function(x, y, width, height) {
      commands.push(["clearRect", number(x), number(y), number(width), number(height)]);
      serialize();
    };
    return context;
  };
  ref.addEventListener = function(type, handler) {
    if (typeof handler !== "function") {
      throw new Error("event handler must be a function");
    }
    __webby_event_handlers.push({
      id: id,
      event_type: String(type),
      handler: String(handler)
    });
  };
  ref.appendChild = function(child) {
    return __webby_append_child(id, child);
  };
  ref.remove = function() {
    __webby_remove(id);
  };
  ref.setAttribute = function(name, value) {
    __webby_set_attribute(id, name, value);
  };
  ref.getAttribute = function(name) {
    return __webby_get_attribute(id, name);
  };
  ref.matches = function(selector) {
    var groups = __webby_split_selector_groups(String(selector));
    for (var groupIndex = 0; groupIndex < groups.length; groupIndex = groupIndex + 1) {
      var steps = __webby_selector_steps(groups[groupIndex]);
      if (__webby_matches_complex_selector(__webby_nodes[id], steps, steps.length - 1)) {
        return true;
      }
    }
    return false;
  };
  ref.closest = function(selector) {
    var node = __webby_nodes[id];
    while (node) {
      if (__webby_ref(node.id).matches(selector)) {
        return __webby_ref(node.id);
      }
      node = __webby_nodes[node.parent];
    }
    return null;
  };
  ref.querySelector = function(selector) {
    return __webby_query_selector_from(id, selector, true);
  };
  ref.querySelectorAll = function(selector) {
    return __webby_query_selector_from(id, selector, false);
  };
  ref.getBoundingClientRect = function() {
    return __webby_dom_rect(id);
  };
  Object.defineProperty(ref, "clientWidth", {
    get: function() {
      return Math.floor(__webby_dom_rect(id).width);
    }
  });
  Object.defineProperty(ref, "clientHeight", {
    get: function() {
      return Math.floor(__webby_dom_rect(id).height);
    }
  });
  Object.defineProperty(ref, "scrollWidth", {
    get: function() {
      return Math.floor(__webby_dom_rect(id).width);
    }
  });
  Object.defineProperty(ref, "scrollHeight", {
    get: function() {
      return Math.floor(__webby_dom_rect(id).height);
    }
  });
  Object.defineProperty(ref, "textContent", {
    get: function() {
      return __webby_text_content(__webby_nodes[id]);
    },
    set: function(value) {
      __webby_set_text_content(id, String(value));
    }
  });
  Object.defineProperty(ref, "innerText", {
    get: function() {
      return __webby_text_content(__webby_nodes[id]);
    },
    set: function(value) {
      __webby_set_text_content(id, String(value));
    }
  });
  Object.defineProperty(ref, "className", {
    get: function() {
      return __webby_get_attribute(id, "class") || "";
    },
    set: function(value) {
      __webby_set_attribute(id, "class", value);
    }
  });
  Object.defineProperty(ref, "classList", {
    get: function() {
      return __webby_make_class_list(id);
    }
  });
  Object.defineProperty(ref, "dataset", {
    get: function() {
      return __webby_make_dataset(id);
    }
  });
  Object.defineProperty(ref, "style", {
    get: function() {
      return __webby_make_style(id);
    }
  });
  Object.defineProperty(ref, "children", {
    get: function() {
      return __webby_child_refs(__webby_nodes[id], true);
    }
  });
  Object.defineProperty(ref, "childNodes", {
    get: function() {
      return __webby_child_refs(__webby_nodes[id], false);
    }
  });
  Object.defineProperty(ref, "parentNode", {
    get: function() {
      var node = __webby_nodes[id];
      return node && node.parent !== null ? __webby_ref(node.parent) : null;
    }
  });
  Object.defineProperty(ref, "firstChild", {
    get: function() {
      var node = __webby_nodes[id];
      return node && node.children.length > 0 ? __webby_ref(node.children[0].id) : null;
    }
  });
  Object.defineProperty(ref, "lastChild", {
    get: function() {
      var node = __webby_nodes[id];
      return node && node.children.length > 0 ? __webby_ref(node.children[node.children.length - 1].id) : null;
    }
  });
  Object.defineProperty(ref, "nextSibling", {
    get: function() {
      return __webby_sibling_ref(id, 1);
    }
  });
  Object.defineProperty(ref, "previousSibling", {
    get: function() {
      return __webby_sibling_ref(id, -1);
    }
  });
  Object.defineProperty(ref, "id", {
    get: function() {
      return __webby_get_attribute(id, "id") || "";
    },
    set: function(value) {
      __webby_set_attribute(id, "id", value);
    }
  });
  Object.defineProperty(ref, "onclick", {
    get: function() {
      return null;
    },
    set: function(handler) {
      if (typeof handler !== "function") {
        throw new Error("event handler must be a function");
      }
      __webby_event_handlers.push({
        id: id,
        event_type: "click",
        handler: String(handler)
      });
    }
  });
  return ref;
}

function __webby_walk(node, callback) {
  if (!node) {
    return null;
  }
  if (callback(node)) {
    return node;
  }
  for (var index = 0; index < node.children.length; index = index + 1) {
    var found = __webby_walk(node.children[index], callback);
    if (found) {
      return found;
    }
  }
  return null;
}

function __webby_has_class(node, className) {
  var classes = node.attributes["class"] || "";
  var parts = classes.split(/\s+/);
  for (var index = 0; index < parts.length; index = index + 1) {
    if (parts[index] === className) {
      return true;
    }
  }
  return false;
}

function __webby_matches_simple_selector(node, selector) {
  if (!node || node.kind !== "element") {
    return false;
  }
  selector = String(selector).trim();
  if (selector === "*") {
    return true;
  }
  if (selector.charAt(0) === "#") {
    return node.attributes.id === selector.slice(1);
  }
  var bracketIndex = selector.indexOf("[");
  if (bracketIndex > 0 && selector.charAt(selector.length - 1) === "]") {
    return node.tag_name === selector.slice(0, bracketIndex).toLowerCase()
      && __webby_matches_simple_selector(node, selector.slice(bracketIndex));
  }
  if (selector.charAt(0) === ".") {
    var wanted = selector.slice(1).split(".");
    for (var classIndex = 0; classIndex < wanted.length; classIndex = classIndex + 1) {
      if (!__webby_has_class(node, wanted[classIndex])) {
        return false;
      }
    }
    return true;
  }
  var hashIndex = selector.indexOf("#");
  if (hashIndex > 0) {
    return node.tag_name === selector.slice(0, hashIndex).toLowerCase()
      && node.attributes.id === selector.slice(hashIndex + 1);
  }
  var dotIndex = selector.indexOf(".");
  if (dotIndex > 0) {
    if (node.tag_name !== selector.slice(0, dotIndex).toLowerCase()) {
      return false;
    }
    return __webby_matches_simple_selector(node, selector.slice(dotIndex));
  }
  if (selector.charAt(0) === "[" && selector.charAt(selector.length - 1) === "]") {
    var attr = selector.slice(1, selector.length - 1);
    var equalsIndex = attr.indexOf("=");
    if (equalsIndex < 0) {
      return Object.prototype.hasOwnProperty.call(node.attributes, attr.toLowerCase());
    }
    var name = attr.slice(0, equalsIndex).toLowerCase();
    var value = attr.slice(equalsIndex + 1).replace(/^"|"$/g, "");
    return node.attributes[name] === value;
  }
  return node.tag_name === selector.toLowerCase();
}

function __webby_split_selector_groups(selector) {
  var groups = [];
  var current = "";
  var bracketDepth = 0;
  var quote = "";
  for (var index = 0; index < selector.length; index = index + 1) {
    var ch = selector.charAt(index);
    if (quote) {
      current = current + ch;
      if (ch === quote) {
        quote = "";
      }
    } else if (ch === '"' || ch === "'") {
      quote = ch;
      current = current + ch;
    } else if (ch === "[") {
      bracketDepth = bracketDepth + 1;
      current = current + ch;
    } else if (ch === "]") {
      bracketDepth = Math.max(0, bracketDepth - 1);
      current = current + ch;
    } else if (ch === "," && bracketDepth === 0) {
      if (current.trim() === "") {
        throw new Error("malformed querySelector selector: " + selector);
      }
      groups.push(current.trim());
      current = "";
    } else {
      current = current + ch;
    }
  }
  if (current.trim() === "") {
    throw new Error("malformed querySelector selector: " + selector);
  }
  groups.push(current.trim());
  return groups;
}

function __webby_selector_steps(selector) {
  if (selector.indexOf("+") >= 0 || selector.indexOf("~") >= 0 || selector.indexOf(":") >= 0) {
    throw new Error("unsupported querySelector selector: " + selector);
  }
  var steps = [];
  var current = "";
  var pendingCombinator = null;
  var bracketDepth = 0;
  var quote = "";
  function pushCurrent() {
    var trimmed = current.trim();
    if (trimmed !== "") {
      steps.push({
        selector: trimmed,
        combinator: steps.length === 0 ? null : (pendingCombinator || "descendant")
      });
      current = "";
      pendingCombinator = null;
    }
  }
  for (var index = 0; index < selector.length; index = index + 1) {
    var ch = selector.charAt(index);
    if (quote) {
      current = current + ch;
      if (ch === quote) {
        quote = "";
      }
    } else if (ch === '"' || ch === "'") {
      quote = ch;
      current = current + ch;
    } else if (ch === "[") {
      bracketDepth = bracketDepth + 1;
      current = current + ch;
    } else if (ch === "]") {
      bracketDepth = Math.max(0, bracketDepth - 1);
      current = current + ch;
    } else if (ch === ">" && bracketDepth === 0) {
      pushCurrent();
      pendingCombinator = "child";
    } else if (/\s/.test(ch) && bracketDepth === 0) {
      if (current.trim() !== "") {
        pushCurrent();
        pendingCombinator = pendingCombinator || "descendant";
      }
    } else {
      current = current + ch;
    }
  }
  pushCurrent();
  if (steps.length === 0 || pendingCombinator === "child") {
    throw new Error("malformed querySelector selector: " + selector);
  }
  return steps;
}

function __webby_matches_complex_selector(node, steps, stepIndex) {
  if (!__webby_matches_simple_selector(node, steps[stepIndex].selector)) {
    return false;
  }
  if (stepIndex === 0) {
    return true;
  }
  var combinator = steps[stepIndex].combinator;
  var parent = __webby_nodes[node.parent];
  if (combinator === "child") {
    return parent ? __webby_matches_complex_selector(parent, steps, stepIndex - 1) : false;
  }
  while (parent) {
    if (__webby_matches_complex_selector(parent, steps, stepIndex - 1)) {
      return true;
    }
    parent = __webby_nodes[parent.parent];
  }
  return false;
}

function __webby_query_selector(selector) {
  return __webby_query_selector_from(__webby_dom_root.id, selector, true);
}

function __webby_query_selector_from(rootId, selector, firstOnly) {
  selector = String(selector).trim();
  var groups = __webby_split_selector_groups(selector);
  var parsedGroups = [];
  for (var groupIndex = 0; groupIndex < groups.length; groupIndex = groupIndex + 1) {
    parsedGroups.push(__webby_selector_steps(groups[groupIndex]));
  }
  var output = [];
  var found = __webby_walk(__webby_nodes[rootId], function(node) {
    for (var groupIndex = 0; groupIndex < parsedGroups.length; groupIndex = groupIndex + 1) {
      var steps = parsedGroups[groupIndex];
      if (__webby_matches_complex_selector(node, steps, steps.length - 1)) {
        if (firstOnly) {
          return true;
        }
        output.push(__webby_ref(node.id));
        return false;
      }
    }
    return false;
  });
  return firstOnly ? (found ? __webby_ref(found.id) : null) : output;
}

__webby_install(__webby_dom_root, null);

var document = {
  getElementById: function(id) {
    var wanted = String(id);
    var found = __webby_walk(__webby_dom_root, function(node) {
      return node.kind === "element" && node.attributes.id === wanted;
    });
    return found ? __webby_ref(found.id) : null;
  },
  querySelector: function(selector) {
    return __webby_query_selector(selector);
  },
  querySelectorAll: function(selector) {
    return __webby_query_selector_from(__webby_dom_root.id, selector, false);
  },
  createElement: function(tagName) {
    var id = __webby_new_temp_id();
    var node = {
      id: id,
      kind: "element",
      tag_name: String(tagName).toLowerCase(),
      text: "",
      attributes: {},
      children: [],
      parent: null
    };
    __webby_nodes[id] = node;
    __webby_mutations.push({ op: "createElement", id: id, tag_name: node.tag_name });
    return __webby_ref(id);
  },
  createTextNode: function(text) {
    var id = __webby_new_temp_id();
    var node = {
      id: id,
      kind: "text",
      tag_name: "",
      text: String(text),
      attributes: {},
      children: [],
      parent: null
    };
    __webby_nodes[id] = node;
    __webby_mutations.push({ op: "createTextNode", id: id, text: node.text });
    return __webby_ref(id);
  }
};

Object.defineProperty(document, "documentElement", {
  get: function() {
    for (var index = 0; index < __webby_dom_root.children.length; index = index + 1) {
      var child = __webby_dom_root.children[index];
      if (child.kind === "element" && child.tag_name === "html") {
        return __webby_ref(child.id);
      }
    }
    return null;
  }
});

Object.defineProperty(document, "body", {
  get: function() {
    var found = __webby_walk(__webby_dom_root, function(node) {
      return node.kind === "element" && node.tag_name === "body";
    });
    return found ? __webby_ref(found.id) : null;
  }
});

Object.defineProperty(document, "title", {
  get: function() {
    var found = __webby_walk(__webby_dom_root, function(node) {
      return node.kind === "element" && node.tag_name === "title";
    });
    return found ? __webby_text_content(found) : "";
  }
});

Object.defineProperty(window, "innerWidth", {
  get: function() {
    return __webby_viewport_width;
  }
});

Object.defineProperty(window, "innerHeight", {
  get: function() {
    return __webby_viewport_height;
  }
});
