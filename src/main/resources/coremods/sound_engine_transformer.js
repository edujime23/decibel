function initializeCoreMod() {
    return {
        'decibel_sound_engine_transformer': {
            'target': {
                'type': 'CLASS',
                'name': 'net.minecraft.client.sounds.SoundEngine'
            },
            'transformer': function(classNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode'); // FIXED: was .type.
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var JumpInsnNode = Java.type('org.objectweb.asm.tree.JumpInsnNode');
                var LabelNode = Java.type('org.objectweb.asm.tree.LabelNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Transforming net.minecraft.client.sounds.SoundEngine...");

                for (var i = 0; i < classNode.methods.size(); i++) {
                    var m = classNode.methods.get(i);
                    var name = m.name;
                    var desc = m.desc;

                    if (name === "play" && desc === "(Lnet/minecraft/client/resources/sounds/SoundInstance;)V") {
                        var instructions = new InsnList();
                        instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        instructions.add(new MethodInsnNode(
                            Opcodes.INVOKESTATIC,
                            "com/edujime23/decibel/asm/SoundInterceptor",
                            "onPlaySound",
                            "(Lnet/minecraft/client/resources/sounds/SoundInstance;)Z",
                            false
                        ));

                        var label = new LabelNode();
                        instructions.add(new JumpInsnNode(Opcodes.IFEQ, label));
                        instructions.add(new InsnNode(Opcodes.RETURN));
                        instructions.add(label);
                        m.instructions.insert(instructions);
                    } else if (name === "stop" && desc === "(Lnet/minecraft/client/resources/sounds/SoundInstance;)V") {
                        var instructions = new InsnList();
                        instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        instructions.add(new MethodInsnNode(
                            Opcodes.INVOKESTATIC,
                            "com/edujime23/decibel/asm/SoundInterceptor",
                            "onStopSound",
                            "(Lnet/minecraft/client/resources/sounds/SoundInstance;)Z",
                            false
                        ));

                        var label = new LabelNode();
                        instructions.add(new JumpInsnNode(Opcodes.IFEQ, label));
                        instructions.add(new InsnNode(Opcodes.RETURN));
                        instructions.add(label);
                        m.instructions.insert(instructions);
                    } else if (name === "updateSource" && desc === "(Lnet/minecraft/client/Camera;)V") {
                        var instructions = new InsnList();
                        instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        instructions.add(new MethodInsnNode(
                            Opcodes.INVOKESTATIC,
                            "com/edujime23/decibel/asm/SoundInterceptor",
                            "onUpdateListener",
                            "(Lnet/minecraft/client/Camera;)V",
                            false
                        ));
                        m.instructions.insert(instructions);
                    } else if ((name === "stopAll" || name === "destroy") && desc === "()V") {
                        var instructions = new InsnList();
                        instructions.add(new MethodInsnNode(
                            Opcodes.INVOKESTATIC,
                            "com/edujime23/decibel/asm/SoundInterceptor",
                            "onStopAll",
                            "()V",
                            false
                        ));
                        m.instructions.insert(instructions);
                    }
                }
                return classNode;
            }
        }
    };
}