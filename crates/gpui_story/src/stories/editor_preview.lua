local Inventory = {}
Inventory.__index = Inventory

function Inventory.new(capacity)
  return setmetatable({ capacity = capacity, items = {}, count = 0 }, Inventory)
end

function Inventory:add(name, quantity)
  quantity = quantity or 1
  if self.count + quantity > self.capacity then
    return false, "inventory is full"
  end
  self.items[name] = (self.items[name] or 0) + quantity
  self.count = self.count + quantity
  return true
end

function Inventory:remove(name, quantity)
  local held = self.items[name]
  if not held or held < quantity then
    return false, "not enough " .. name
  end
  self.items[name] = held - quantity
  self.count = self.count - quantity
  return true
end

function Inventory:report()
  local names = {}
  for name in pairs(self.items) do
    names[#names + 1] = name
  end
  table.sort(names)
  for _, name in ipairs(names) do
    print(string.format("%-10s x%d", name, self.items[name]))
  end
end

local bag = Inventory.new(20)
assert(bag:add("potion", 3))
assert(bag:add("arrow", 12))
local ok, err = bag:add("sword", 10)
if not ok then
  print("could not add sword: " .. err)
end
bag:report()
